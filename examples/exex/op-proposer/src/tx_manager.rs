use alloy_network::Network;
use alloy_provider::{PendingTransaction, Provider};
use alloy_transport::Transport;
use futures::Future;
use parking_lot::Mutex;
use reth_primitives::{Bytes, B256, U256};
use reth_tracing::tracing::{info, warn};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::op_proposer::{
    DisputeGameFactory::DisputeGameFactoryInstance, L2Output,
    L2OutputOracle::L2OutputOracleInstance,
};

pub struct TxManager<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    // Hashset to keep track of which L2Outputs have been proposed, keyed by l2_block_number
    pub pending_transactions: Arc<Mutex<HashSet<u64>>>,
    pub pending_transaction_tx: Sender<(u64, PendingTransaction)>,
    pub l2_output_oracle: Arc<L2OutputOracleInstance<T, Arc<P>, N>>,
}

impl<T, N, P> TxManager<T, N, P>
where
    T: Transport + Clone,
    N: Network,
    P: Provider<T, N>,
{
    pub fn new(
        l2_output: Arc<L2OutputOracleInstance<T, Arc<P>, N>>,
        pending_transaction_tx: Sender<(u64, PendingTransaction)>,
    ) -> Self {
        Self {
            pending_transactions: Arc::new(Mutex::new(HashSet::new())),
            l2_output_oracle: l2_output,
            pending_transaction_tx,
        }
    }

    /// Propose an L2Output to the L2OutputOracle contract. Pending transactions are added to the
    /// `pending_transactions` HashSet and the TxManager will wait for the transaction to complete
    /// asynchronously.
    pub async fn propose_l2_output(
        &mut self,
        l2_output_oracle: &L2OutputOracleInstance<T, Arc<P>, N>,
        l2_output: L2Output,
    ) -> eyre::Result<()> {
        self.pending_transactions.lock().insert(l2_output.l2_block_number);

        // Submit a transaction to propose the L2Output to the L2OutputOracle contract
        let transport_result = l2_output_oracle
            .proposeL2Output(
                l2_output.output_root,
                U256::from(l2_output.l2_block_number),
                l2_output.l1_block_hash,
                U256::from(l2_output.l1_block_number),
            )
            .send()
            .await?
            .register()
            .await?;

        self.pending_transaction_tx.send((l2_output.l2_block_number, transport_result)).await?;

        info!(
            output_root = ?l2_output.output_root,
            l2_block_number = ?l2_output.l2_block_number,
            l1_block_hash = ?l2_output.l1_block_hash,
            l1_block_number = ?l2_output.l1_block_number,
            "Proposing L2Output"
        );

        Ok(())
    }

    pub async fn create_dispute_game(
        &mut self,
        dispute_game_factory: &DisputeGameFactoryInstance<T, Arc<P>, N>,
        game_type: u32,
        root_claim: B256,
        l2_block_number: u64,
    ) -> eyre::Result<()> {
        self.pending_transactions.lock().insert(l2_block_number);

        let init_bond = dispute_game_factory.initBonds(game_type).call().await?;

        let transport_result = dispute_game_factory
            .create(game_type, root_claim, Bytes::new())
            .value(U256::from(init_bond._0))
            .send()
            .await?
            .register()
            .await?;

        self.pending_transaction_tx.send((l2_block_number, transport_result)).await?;
        info!(?game_type, ?root_claim, ?l2_block_number, "Creating Dispute Game");

        Ok(())
    }

    /// Core logic of the TxManager. This function will listen for pending transactions send through
    /// the `pending_transaction_tx` and wait for them to complete. Once the transaction has
    /// succeeded or failed, the transaction will be removed from the `pending_transactions`
    /// HashSet.
    pub fn run(
        &mut self,
        mut pending_rx: Receiver<(u64, PendingTransaction)>,
    ) -> impl Future<Output = eyre::Result<()>> {
        let pending_transactions = self.pending_transactions.clone();

        async move {
            loop {
                if let Some((l2_block_number, pending_transaction)) = pending_rx.recv().await {
                    match pending_transaction.await {
                        Ok(_) => {
                            info!(l2_block_number, "L2Output proposed successfully");
                        }
                        Err(e) => {
                            warn!(error = ?e, "Error while proposing L2Output")
                        }
                    };

                    // Once the pending transaction has completed, remove it from the pending
                    // regardless if the tx succeeded or failed
                    // If the transaction did not successfully complete, the l2_oracle.nextBlock
                    // will be the same and the next transaction will propose the same l2_output
                    pending_transactions.lock().remove(&l2_block_number);
                }
            }
        }
    }
}
