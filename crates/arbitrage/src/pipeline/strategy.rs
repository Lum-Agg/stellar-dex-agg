//! Fan-out bridge scan items to parallel quote workers.

use {
    crate::{
        pipeline::types::{Action, BridgeScanItem, Event},
        runtime::SharedRuntime,
        scanner::evaluate_bridge_pair,
    },
    anyhow::Result,
    async_channel::{Receiver, Sender},
    async_trait::async_trait,
    burberry::{ActionSubmitter, Strategy},
    std::sync::Arc,
    tracing::{error, info},
};

pub struct BridgeStrategy {
    runtime: SharedRuntime,
    item_sender: Option<Sender<BridgeScanItem>>,
    worker_count: usize,
}

impl BridgeStrategy {
    pub fn new(runtime: SharedRuntime, worker_count: usize) -> Self {
        Self {
            runtime,
            item_sender: None,
            worker_count,
        }
    }
}

#[async_trait]
impl Strategy<Event, Action> for BridgeStrategy {
    fn name(&self) -> &str {
        "BridgeStrategy"
    }

    async fn sync_state(&mut self, submitter: Arc<dyn ActionSubmitter<Action>>) -> Result<()> {
        if self.item_sender.is_some() {
            anyhow::bail!("BridgeStrategy already synced");
        }

        // Buffer at most one scan cycle. Pairs repeat every cycle, so allowing
        // duplicates to accumulate would only delay the next useful scan.
        let queue_capacity = self
            .runtime
            .config
            .base_tokens
            .len()
            .saturating_mul(self.runtime.config.bridge_tokens.len())
            .clamp(1, 100);
        let (tx, rx) = async_channel::bounded(queue_capacity);
        self.item_sender = Some(tx.clone());

        for id in 0..self.worker_count {
            spawn_worker(id, self.runtime.clone(), rx.clone(), submitter.clone());
        }

        info!(
            workers = self.worker_count,
            queue_capacity, "bridge quote workers spawned"
        );
        Ok(())
    }

    async fn process_event(&mut self, event: Event, _submitter: Arc<dyn ActionSubmitter<Action>>) {
        let Event::BridgeScan(item) = event;
        if let Some(sender) = &self.item_sender {
            match sender.try_send(item) {
                Ok(()) | Err(async_channel::TrySendError::Full(_)) => {}
                Err(async_channel::TrySendError::Closed(_)) => {
                    error!("bridge scan channel closed");
                }
            }
        }
    }
}

fn spawn_worker(
    id: usize,
    runtime: SharedRuntime,
    rx: Receiver<BridgeScanItem>,
    submitter: Arc<dyn ActionSubmitter<Action>>,
) {
    tokio::spawn(async move {
        info!(worker.id = id, "bridge worker started");
        while let Ok(item) = rx.recv().await {
            if let Err(e) = handle_item(&runtime, item, submitter.as_ref()).await {
                error!(worker.id = id, error = %e, "bridge worker item failed");
            }
        }
    });
}

async fn handle_item(
    runtime: &SharedRuntime,
    item: BridgeScanItem,
    submitter: &dyn ActionSubmitter<Action>,
) -> Result<()> {
    let ctx = runtime.connect().await?;
    let Some(opp) = evaluate_bridge_pair(&ctx, &item.base, &item.bridge, &runtime.stats).await? else {
        return Ok(());
    };

    if runtime.build_enabled() {
        submitter.submit(Action::ExecuteOpportunity(opp));
    }

    Ok(())
}
