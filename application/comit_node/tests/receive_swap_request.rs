extern crate tokio;
extern crate transport_protocol;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate maplit;
#[macro_use]
extern crate serde_json;
extern crate bitcoin_rpc_client;
extern crate bitcoin_support;
extern crate comit_node;
extern crate comit_wallet;
extern crate common_types;
extern crate ethereum_support;
extern crate ethereum_wallet;
extern crate event_store;
extern crate failure;
extern crate futures;
extern crate hex;
extern crate memsocket;
extern crate pretty_env_logger;
extern crate reqwest;
extern crate secp256k1_support;
extern crate serde;
extern crate spectral;
extern crate tc_web3_client;
extern crate testcontainers;
extern crate web3;

mod mocks;
use bitcoin_support::{Address as BitcoinAddress, BitcoinQuantity, Blocks, TransactionId};
use comit_node::{
    bitcoin_fee_service::StaticBitcoinFeeService,
    gas_price_service::StaticGasPriceService,
    ledger_query_service::fake_query_service::SimpleFakeLedgerQueryService,
    swap_protocols::{
        json_config,
        ledger::{bitcoin::Bitcoin, ethereum::Ethereum},
        rfc003::{
            self,
            ledger_htlc_service::{BitcoinService, EthereumService},
        },
        wire_types::SwapResponse,
        SwapRequestHandler,
    },
};
use comit_wallet::fake_key_store::FakeKeyStoreFactory;
use ethereum_support::EthereumQuantity;
use ethereum_wallet::fake::StaticFakeWallet;
use event_store::InMemoryEventStore;
use futures::future::Future;
use hex::FromHex;
use mocks::BitcoinRpcClientMock;
use secp256k1_support::KeyPair;
use spectral::prelude::*;
use std::{str::FromStr, sync::Arc, time::Duration};
use testcontainers::{clients::Cli, images::parity_parity::ParityEthereum, Docker};
use tokio::runtime::Runtime;
use transport_protocol::{
    client::*,
    config::Config,
    connection::*,
    json::*,
    shutdown_handle::{self, ShutdownHandle},
    Status,
};

fn setup<
    H: SwapRequestHandler<rfc003::Request<Bitcoin, Ethereum, BitcoinQuantity, EthereumQuantity>>
        + SwapRequestHandler<rfc003::Request<Ethereum, Bitcoin, EthereumQuantity, BitcoinQuantity>>,
>(
    swap_request_handler: H,
    btc_redeem_pubkeyhash: bitcoin_support::PubkeyHash,
) -> (
    Runtime,
    Client<Frame, Request, Response>,
    Client<Frame, Request, Response>,
    ShutdownHandle,
    ShutdownHandle,
) {
    let (alice, bob) = memsocket::unbounded();
    let mut runtime = Runtime::new().unwrap();

    let (alice_server, bob_client) =
        Connection::new(Config::default(), JsonFrameCodec::default(), alice)
            .start::<JsonFrameHandler>();
    let (alice_server, alice_shutdown_handle) = shutdown_handle::new(alice_server);

    let ledger_query_service = Arc::new(SimpleFakeLedgerQueryService {
        results: vec![
            "7e7c52b1f46e7ea2511e885d8c0e5df9297f65b6fff6907ceb1377d0582e45f4"
                .parse()
                .unwrap(),
        ],
    });
    let docker = Cli::default();

    let container = docker.run(ParityEthereum::default());

    let alice_keypair = KeyPair::from_secret_key_hex(
        "63be4b0d638d44b5fee5b050ab0beeeae7b68cde3d829a3321f8009cdd76b992",
    ).unwrap();

    let ethereum_service = Arc::new(EthereumService::new(
        Arc::new(StaticFakeWallet::from_key_pair(alice_keypair.clone())),
        Arc::new(StaticGasPriceService::default()),
        Arc::new(tc_web3_client::new(&container)),
        0,
    ));

    let bitcoin_fee_service = Arc::new(StaticBitcoinFeeService::new(50.0));

    let btc_redeem_address = BitcoinAddress::from_pubkeyhash_and_network(
        btc_redeem_pubkeyhash,
        bitcoin_support::Network::Regtest,
    );

    let bitcoin_service = Arc::new(BitcoinService::new(
        Arc::new(BitcoinRpcClientMock::new(
            TransactionId::from_str(
                "d54994ece1d11b19785c7248868696250ab195605b469632b7bd68130e880c9a",
            ).unwrap(),
        )),
        bitcoin_support::Network::Regtest,
        bitcoin_fee_service.clone(),
        btc_redeem_address,
    ));

    let (bob_server, alice_client) = Connection::new(
        json_config(
            swap_request_handler,
            Arc::new(FakeKeyStoreFactory::create()),
            Arc::new(InMemoryEventStore::default()),
            ledger_query_service,
            ethereum_service,
            bitcoin_service,
            bitcoin_support::Network::Regtest,
            Duration::from_secs(1),
        ),
        JsonFrameCodec::default(),
        bob,
    ).start::<JsonFrameHandler>();
    let (bob_server, bob_shutdown_handle) = shutdown_handle::new(bob_server);

    runtime.spawn(alice_server.map_err(|_| ()));
    runtime.spawn(bob_server.map_err(|_| ()));

    (
        runtime,
        alice_client,
        bob_client,
        alice_shutdown_handle,
        bob_shutdown_handle,
    )
}

#[derive(PartialEq)]
enum OfferDirection {
    BtcToEth,
    EthToBtc,
}

lazy_static! {
    static ref BTC_REFUND_PUBKEYHASH: bitcoin_support::PubkeyHash =
        bitcoin_support::PubkeyHash::from_hex("875638cac0b0ae9f826575e190f2788918c354c2").unwrap();
    static ref BTC_SUCCESS_PUBKEYHASH: bitcoin_support::PubkeyHash =
        bitcoin_support::PubkeyHash::from_hex("30bfdb95f68bfdd558a8dc6deef0da882b0c4866").unwrap();
    static ref ETH_REFUND_ADDRESS: ethereum_support::Address =
        ethereum_support::Address::from_str("8457037fcd80a8650c4692d7fcfc1d0a96b92867").unwrap();
    static ref ETH_SUCCESS_ADDRESS: ethereum_support::Address =
        ethereum_support::Address::from_str("0ae91a668e3ad094e765ec66f5d5c72e0b82f04d").unwrap();
}

fn gen_request(direction: OfferDirection) -> Request {
    let bitcoin = json!("Bitcoin");
    let ethereum = json!("Ethereum");
    let bitcoin_asset = json!({
            "value": "Bitcoin",
            "parameters": {
                "quantity": "100000000",
            }
    });
    let ethereum_asset = json!({
        "value": "Ether",
        "parameters": {
            "quantity": "10000000000000000000",
        }
    });

    let (
        source_ledger,
        target_ledger,
        source_asset,
        target_asset,
        source_ledger_refund_identity,
        target_ledger_success_identity,
    ) = match direction {
        OfferDirection::BtcToEth => (
            bitcoin,
            ethereum,
            bitcoin_asset,
            ethereum_asset,
            hex::encode(BTC_REFUND_PUBKEYHASH.clone()),
            format!("0x{}", hex::encode(ETH_SUCCESS_ADDRESS.clone())),
        ),
        OfferDirection::EthToBtc => (
            ethereum,
            bitcoin,
            ethereum_asset,
            bitcoin_asset,
            format!("0x{}", hex::encode(ETH_REFUND_ADDRESS.clone())),
            hex::encode(BTC_SUCCESS_PUBKEYHASH.clone()),
        ),
    };

    let headers = convert_args!(hashmap!(
        "source_ledger" => source_ledger,
        "target_ledger" => target_ledger,
        "source_asset" => source_asset,
        "target_asset" => target_asset,
        "swap_protocol" => json!("COMIT-RFC-003"),
    ));

    let body = json!({
        "source_ledger_refund_identity": source_ledger_refund_identity,
        "target_ledger_success_identity": target_ledger_success_identity,
        "source_ledger_lock_duration": 144,
        "secret_hash": "f6fc84c9f21c24907d6bee6eec38cabab5fa9a7be8c4a7827fe9e56f245bd2d5"
    });

    println!("{}", serde_json::to_string(&body).unwrap());

    Request::new("SWAP".into(), headers, body)
}

#[test]
fn can_receive_swap_request() {
    struct CaptureSwapMessage {
        sender: Option<
            futures::sync::oneshot::Sender<
                rfc003::Request<Bitcoin, Ethereum, BitcoinQuantity, EthereumQuantity>,
            >,
        >,
    }

    impl SwapRequestHandler<rfc003::Request<Bitcoin, Ethereum, BitcoinQuantity, EthereumQuantity>>
        for CaptureSwapMessage
    {
        fn handle(
            &mut self,
            request: rfc003::Request<Bitcoin, Ethereum, BitcoinQuantity, EthereumQuantity>,
        ) -> SwapResponse {
            self.sender.take().unwrap().send(request).unwrap();
            SwapResponse::Decline
        }
    }

    impl SwapRequestHandler<rfc003::Request<Ethereum, Bitcoin, EthereumQuantity, BitcoinQuantity>>
        for CaptureSwapMessage
    {}

    let _ = pretty_env_logger::try_init();

    let (sender, receiver) = futures::sync::oneshot::channel();

    let handler = CaptureSwapMessage {
        sender: Some(sender),
    };

    let (_runtime, _to_alice, mut to_bob, _alice_handle, _bob_handle) =
        setup(handler, *BTC_SUCCESS_PUBKEYHASH);

    let _response = to_bob
        .send_request(gen_request(OfferDirection::BtcToEth))
        .wait();

    assert_that(&_response)
        .is_ok()
        .map(|r| r.status())
        .is_equal_to(Status::SE(21));

    let result = receiver.wait();

    let expected_request = rfc003::Request {
        source_ledger: Bitcoin::regtest(),
        target_ledger: Ethereum::default(),
        source_asset: BitcoinQuantity::from_satoshi(100_000_000),
        target_asset: EthereumQuantity::from_eth(10.0),
        source_ledger_lock_duration: Blocks::from(144),
        source_ledger_refund_identity: BTC_REFUND_PUBKEYHASH.clone(),
        target_ledger_success_identity: ETH_SUCCESS_ADDRESS.clone(),
        secret_hash: "f6fc84c9f21c24907d6bee6eec38cabab5fa9a7be8c4a7827fe9e56f245bd2d5"
            .parse()
            .unwrap(),
    };

    assert_that(&result).is_ok().is_equal_to(&expected_request)
}

struct AcceptRate {
    pub btc_to_eth: f64,
}

impl SwapRequestHandler<rfc003::Request<Bitcoin, Ethereum, BitcoinQuantity, EthereumQuantity>>
    for AcceptRate
{
    fn handle(
        &mut self,
        request: rfc003::Request<Bitcoin, Ethereum, BitcoinQuantity, EthereumQuantity>,
    ) -> SwapResponse {
        let bitcoin = request.source_asset.bitcoin();
        let ethereum = request.target_asset.ethereum();

        let requested_rate = bitcoin / ethereum;
        if requested_rate > self.btc_to_eth {
            SwapResponse::Accept
        } else {
            SwapResponse::Decline
        }
    }
}

impl SwapRequestHandler<rfc003::Request<Ethereum, Bitcoin, EthereumQuantity, BitcoinQuantity>>
    for AcceptRate
{
    fn handle(
        &mut self,
        request: rfc003::Request<Ethereum, Bitcoin, EthereumQuantity, BitcoinQuantity>,
    ) -> SwapResponse {
        let bitcoin = request.target_asset.bitcoin();
        let ethereum = request.source_asset.ethereum();

        let requested_rate = bitcoin / ethereum;
        if requested_rate < self.btc_to_eth {
            SwapResponse::Accept
        } else {
            SwapResponse::Decline
        }
    }
}

#[test]
fn rate_handler_reject_offer_btc_eth() {
    // The offer gives 1 BTC in exchange 10 ETH
    // But I am only willing to spend 5 ETH for a BTC
    // so REJECT
    let handler = AcceptRate {
        btc_to_eth: 1.0 / 5.0,
    };

    let (_runtime, _to_alice, mut to_bob, _alice_handle, _bob_handle) =
        setup(handler, *BTC_SUCCESS_PUBKEYHASH);
    let response = to_bob
        .send_request(gen_request(OfferDirection::BtcToEth))
        .wait();

    assert_that(&response)
        .is_ok()
        .map(|r| r.status())
        .is_equal_to(Status::SE(21));
}

#[test]
fn rate_handler_accept_offer_btc_eth() {
    // The offer gives 1 BTC in exchange 10 ETH
    // I am willing to give at most 11 ETH for 1 BTC
    // so ACCEPT
    let handler = AcceptRate {
        btc_to_eth: 1.0 / 11.0,
    };
    let (_runtime, _to_alice, mut to_bob, _alice_handle, _bob_handle) =
        setup(handler, *BTC_SUCCESS_PUBKEYHASH);
    let response = to_bob
        .send_request(gen_request(OfferDirection::BtcToEth))
        .wait();

    assert_that(&response)
        .is_ok()
        .map(Response::status)
        .is_equal_to(Status::OK(20));
}

#[test]
fn rate_handler_reject_offer_eth_btc() {
    // The offer gives 10 ETH in exchange for 1 BTC
    // I am willing to accept at least 11 ETH for a BTC
    // so REJECT
    let handler = AcceptRate {
        btc_to_eth: 1.0 / 11.0,
    };
    let (_runtime, _to_alice, mut to_bob, _alice_handle, _bob_handle) =
        setup(handler, *BTC_SUCCESS_PUBKEYHASH);
    let response = to_bob
        .send_request(gen_request(OfferDirection::EthToBtc))
        .wait();

    assert_that(&response)
        .is_ok()
        .map(|r| r.status())
        .is_equal_to(Status::SE(21));
}

#[test]
fn rate_handler_accept_offer_eth_btc() {
    // The offer gives 10 ETH for 1 BTC
    // I am willing to accept at least 5 ETH for a BTC
    // so ACCEPT
    let handler = AcceptRate {
        btc_to_eth: 1.0 / 5.0,
    };
    let (_runtime, _to_alice, mut to_bob, _alice_handle, _bob_handle) =
        setup(handler, *BTC_SUCCESS_PUBKEYHASH);
    let response = to_bob
        .send_request(gen_request(OfferDirection::EthToBtc))
        .wait();

    assert_that(&response)
        .is_ok()
        .is_equal_to(&Response::new(Status::SE(22)));
}