use crate::{
    ledger_query_service::EthereumQuery,
    swap_protocols::{
        ledger::Ethereum,
        rfc003::{
            events::{NewHtlcFundedQuery, NewHtlcRedeemedQuery, NewHtlcRefundedQuery},
            state_machine::HtlcParams,
            Secret,
        },
    },
};
use ethereum_support::{web3::types::Address, Bytes, EtherQuantity};

impl NewHtlcFundedQuery<Ethereum, EtherQuantity> for EthereumQuery {
    fn new_htlc_funded_query(htlc_params: &HtlcParams<Ethereum, EtherQuantity>) -> Self {
        EthereumQuery::Transaction {
            from_address: None,
            to_address: None,
            is_contract_creation: Some(true),
            transaction_data: Some(htlc_params.bytecode()),
            transaction_data_length: None,
        }
    }
}

impl NewHtlcRefundedQuery<Ethereum, EtherQuantity> for EthereumQuery {
    fn new_htlc_refunded_query(
        _htlc_params: &HtlcParams<Ethereum, EtherQuantity>,
        htlc_location: &Address,
    ) -> Self {
        EthereumQuery::Transaction {
            from_address: None,
            to_address: Some(*htlc_location),
            is_contract_creation: Some(false),
            transaction_data: Some(Bytes::from(vec![])),
            transaction_data_length: None,
        }
    }
}

impl NewHtlcRedeemedQuery<Ethereum, EtherQuantity> for EthereumQuery {
    fn new_htlc_redeemed_query(
        _htlc_params: &HtlcParams<Ethereum, EtherQuantity>,
        htlc_location: &Address,
    ) -> Self {
        EthereumQuery::Transaction {
            from_address: None,
            to_address: Some(*htlc_location),
            is_contract_creation: Some(false),
            transaction_data: None,
            transaction_data_length: Some(Secret::LENGTH),
        }
    }
}

pub mod erc20 {
    use super::*;
    use ethereum_support::Erc20Quantity;

    pub fn new_htlc_deployed_query(
        htlc_params: &HtlcParams<Ethereum, Erc20Quantity>,
    ) -> EthereumQuery {
        EthereumQuery::Transaction {
            from_address: None,
            to_address: None,
            is_contract_creation: Some(true),
            transaction_data: Some(htlc_params.bytecode()),
            transaction_data_length: None,
        }
    }

    pub fn new_htlc_funded_query(
        htlc_params: &HtlcParams<Ethereum, Erc20Quantity>,
        htlc_location: &Address,
    ) -> EthereumQuery {
        EthereumQuery::Transaction {
            from_address: None,
            to_address: Some(htlc_params.asset.token_contract()),
            is_contract_creation: None,
            transaction_data: Some(htlc_params.funding_tx_payload(*htlc_location)),
            transaction_data_length: None,
        }
    }

    pub fn new_htlc_refunded_query(htlc_location: &Address) -> EthereumQuery {
        EthereumQuery::Transaction {
            from_address: None,
            to_address: Some(*htlc_location),
            is_contract_creation: Some(false),
            transaction_data: Some(Bytes::from(vec![])),
            transaction_data_length: None,
        }
    }

    pub fn new_htlc_redeemed_query(htlc_location: &Address) -> EthereumQuery {
        EthereumQuery::Transaction {
            from_address: None,
            to_address: Some(*htlc_location),
            is_contract_creation: Some(false),
            transaction_data: None,
            transaction_data_length: Some(Secret::LENGTH),
        }
    }
}
