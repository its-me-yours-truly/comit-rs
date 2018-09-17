use query_repository::QueryRepository;
use query_result_repository::QueryResultRepository;
use std::{fmt::Debug, sync::Arc};

pub trait TransactionProcessor<T> {
    fn process(&self, transaction: &T);
}

pub trait Transaction: Debug {
    fn txid(&self) -> String;
}

pub trait Query<T>: Debug {
    fn matches(&self, transaction: &T) -> bool;
}

pub struct DefaultTransactionProcessor<Q> {
    queries: Arc<QueryRepository<Q>>,
    results: Arc<QueryResultRepository>,
}

impl<T: Transaction, Q: Query<T> + 'static> TransactionProcessor<T>
    for DefaultTransactionProcessor<Q>
{
    fn process(&self, transaction: &T) {
        trace!("Processing {:?}", transaction);

        self.queries
            .all()
            .filter(|(_, query)| query.matches(transaction))
            .map(|(id, query)| (id, transaction.txid(), query))
            .inspect(|(id, txid, query)| {
                info!(
                    "Transaction {} matches {:#?} Query-ID: {:?}",
                    txid, query, id
                )
            })
            .for_each(|(query_id, tx_id, _)| self.results.add_result(query_id, tx_id))
    }
}

impl<Q> DefaultTransactionProcessor<Q> {
    pub fn new(
        query_repository: Arc<QueryRepository<Q>>,
        query_result_repository: Arc<QueryResultRepository>,
    ) -> Self {
        Self {
            queries: query_repository,
            results: query_result_repository,
        }
    }
}