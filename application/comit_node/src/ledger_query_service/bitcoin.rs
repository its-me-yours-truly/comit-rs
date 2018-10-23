use bitcoin_support;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum BitcoinQuery {
    Transaction {
        to_address: Option<bitcoin_support::Address>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin_support::Address;
    use serde_json;
    use std::str::FromStr;

    #[test]
    fn given_a_bitcoin_transaction_query_with_toaddress_it_serializes_ok() {
        let to_address =
            Some(Address::from_str("bcrt1qcqslz7lfn34dl096t5uwurff9spen5h4v2pmap").unwrap());
        let query = BitcoinQuery::Transaction { to_address };
        let query = serde_json::to_string(&query).unwrap();
        assert_eq!(
            query,
            r#"{"to_address":"bcrt1qcqslz7lfn34dl096t5uwurff9spen5h4v2pmap"}"#
        )
    }

    #[test]
    fn given_an_empty_bitcoin_transaction_query_it_serializes_ok() {
        let to_address = None;
        let query = BitcoinQuery::Transaction { to_address };
        let query = serde_json::to_string(&query).unwrap();
        assert_eq!(query, r#"{"to_address":null}"#)
    }
}
