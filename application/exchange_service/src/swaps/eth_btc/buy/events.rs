use bitcoin_rpc;
use bitcoin_support;
use common_types::{
    ledger::{bitcoin::Bitcoin, ethereum::Ethereum},
    secret::SecretHash,
};
use ethereum_support::{self, *};
use event_store::Event;
use secp256k1_support::KeyPair;
use std::time::SystemTime;
pub use swaps::eth_btc::common::{OfferCreated as OfferState, OfferCreated};
use swaps::TradeId;
use treasury_api_client::RateResponseBody;
use uuid::Uuid;

impl From<RateResponseBody> for OfferCreated<Ethereum, Bitcoin> {
    fn from(r: RateResponseBody) -> Self {
        OfferCreated {
            uid: TradeId(Uuid::new_v4()),
            symbol: r.symbol,
            rate: r.rate,
            buy_amount: r.buy_amount,   // ETH
            sell_amount: r.sell_amount, // BTC
        }
    }
}

#[derive(Clone)]
pub struct OrderTaken {
    pub uid: TradeId,

    pub contract_secret_lock: SecretHash,
    pub client_contract_time_lock: bitcoin_rpc::BlockHeight,
    pub exchange_contract_time_lock: SystemTime,

    pub client_refund_address: bitcoin_support::Address,
    pub client_success_address: ethereum_support::Address,

    pub exchange_refund_address: ethereum_support::Address,
    pub exchange_success_address: bitcoin_support::Address,
    pub exchange_success_keypair: KeyPair,
}

impl Event for OrderTaken {
    type Prev = OfferCreated<Ethereum, Bitcoin>;
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TradeFunded {
    pub uid: TradeId,
    pub transaction_id: bitcoin_rpc::TransactionId,
    pub vout: u32,
}

impl Event for TradeFunded {
    type Prev = OrderTaken;
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractDeployed {
    pub uid: TradeId,
    pub transaction_id: H256,
}

impl Event for ContractDeployed {
    type Prev = TradeFunded;
}
