use std::{
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use futures::{SinkExt, channel::oneshot};
use tracing::{trace, warn};

use crate::{
    model::proto::KeepaliveRequestData,
    platform::{self, NetworkInterface},
    tunnel::ssl::PacketSender,
};

const KEEPALIVE_MAX_RETRIES: i64 = 3;
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

pub struct KeepaliveRunner {
    interval: Duration,
    sender: PacketSender,
    keepalive_counter: Arc<AtomicI64>,
}

impl KeepaliveRunner {
    pub fn new(interval: Duration, sender: PacketSender, counter: Arc<AtomicI64>) -> Self {
        Self {
            interval,
            sender,
            keepalive_counter: counter,
        }
    }

    pub async fn run(&self) {
        let (stop_sender, stop_receiver) = oneshot::channel();

        let interval = self.interval;
        let keepalive_counter = self.keepalive_counter.clone();
        let mut sender = self.sender.clone();

        tokio::spawn(async move {
            loop {
                if platform::new_network_interface().is_online() {
                    if keepalive_counter.load(Ordering::SeqCst) >= KEEPALIVE_MAX_RETRIES {
                        let msg = "No response for keepalive packets, tunnel appears stuck";
                        warn!(msg);
                        break;
                    }

                    let req = KeepaliveRequestData { id: "0".to_string() };
                    trace!("Keepalive request: {:?}", req);

                    keepalive_counter.fetch_add(1, Ordering::SeqCst);

                    match tokio::time::timeout(SEND_TIMEOUT, sender.send(req.into())).await {
                        Ok(Ok(())) => {}
                        _ => {
                            warn!("Cannot send keepalive packet, exiting");
                            break;
                        }
                    }
                }
                tokio::time::sleep(interval).await;
            }
            let _ = stop_sender.send(());
        });

        let _ = stop_receiver.await;
    }
}
