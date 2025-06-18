use async_trait::async_trait;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Notify};

pub use actor_macro::define_actor;

#[cfg(test)]
mod lib_test;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
pub enum Priority {
    Low,
    Medium,
    High,
    // Add highest priority for shutdown messages
    Shutdown,
}

impl Prioritized for Priority {
    fn priority(&self) -> Priority {
        *self
    }
}

pub trait Prioritized {
    fn priority(&self) -> Priority {
        Priority::Medium
    }
}

#[async_trait]
pub trait Actor: Send + 'static {
    type Msg: Send + 'static + Prioritized;

    async fn handle(&mut self, msg: Self::Msg) -> bool;
}

pub fn spawn_actor<A>(mut actor: A) -> mpsc::Sender<A::Msg>
where
    A: Actor + Send + 'static,
{
    // The channel capacity. Choose based on expected message volume.
    // A smaller buffer might apply backpressure sooner.
    let (tx, mut rx) = mpsc::channel::<A::Msg>(32);

    // Queue for messages, protected by a Mutex, ordered by Priority
    let queue = Arc::new(Mutex::new(BinaryHeap::<PrioritizedWrapper<A::Msg>>::new()));
    // Notify to signal new messages in the queue
    let notify = Arc::new(Notify::new());

    // Fill the queue
    let queue_rx = Arc::clone(&queue);
    let notify_rx = Arc::clone(&notify);
    let actor_name = std::any::type_name::<A>().to_string(); // For logging
    tokio::spawn(async move {
        println!("[{}] Message receiver task started.", actor_name);
        while let Some(msg) = rx.recv().await {
            let mut q = queue_rx.lock().await;
            q.push(PrioritizedWrapper(msg));
            notify_rx.notify_one();
        }
        // rx.recv() returned None, meaning all senders have been dropped.
        // This task can now gracefully terminate.
        println!(
            "[{}] All senders dropped. Message receiver task terminating.",
            actor_name
        );
    });

    // Process messages
    tokio::spawn(async move {
        println!(
            "[{}] Message processor task started.",
            std::any::type_name::<A>()
        );
        loop {
            let msg_opt = {
                // Scoped lock for the queue
                let mut q = queue.lock().await;
                if q.is_empty() {
                    // If the queue is empty, release the lock and wait for a notification.
                    // This allows the receiver task to push new messages without deadlock.
                    drop(q);
                    notify.notified().await;
                    queue.lock().await.pop()
                } else {
                    // If the queue is not empty, pop a message immediately.
                    q.pop()
                }
            };

            if let Some(PrioritizedWrapper(msg)) = msg_opt {
                // If handle returns false, it signals the actor should stop
                if !actor.handle(msg).await {
                    println!(
                        "[{}] Actor received shutdown signal. Processor task terminating.",
                        std::any::type_name::<A>()
                    );
                    break; // Exit the loop on shutdown signal
                }
            } else {
                // `msg_opt` is `None`. This happens when `queue.pop()` returns `None`.
                // This signifies that the message receiver task has terminated
                // (because its `rx.recv().await` returned `None`, meaning all senders were dropped)
                // AND the queue is now empty.
                let q_check = queue.lock().await;
                if q_check.is_empty() {
                    println!("[{}] Message queue empty and no more messages expected. Processor task terminating.", std::any::type_name::<A>());
                    break; // Exit the loop
                }
                // If q_check is *not* empty here, it means we somehow popped None
                // from a non-empty queue, which shouldn't happen with BinaryHeap.
                // This 'else' path primarily catches the true shutdown condition.
            }
        }
    });

    tx
}

pub struct PrioritizedWrapper<T>(pub T);

impl<T: Prioritized> PartialEq for PrioritizedWrapper<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.priority() == other.0.priority()
    }
}

impl<T: Prioritized> Eq for PrioritizedWrapper<T> {}

impl<T: Prioritized> PartialOrd for PrioritizedWrapper<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Prioritized> Ord for PrioritizedWrapper<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.priority().cmp(&self.0.priority())
    }
}
