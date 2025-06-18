//! # priact
//!
//! [![crates.io](https://img.shields.io/crates/v/priact)](https://crates.io/crates/priact)
//!
//! A lightweight, priority-driven actor framework for Rust.
//!
#![doc = include_str!("../README.md")]

use async_trait::async_trait;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Notify};

pub use priact_actor_macro::define_actor;

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

    // Receiver task
    let queue_rx = Arc::clone(&queue);
    let notify_rx = Arc::clone(&notify);
    let actor_name_rx = std::any::type_name::<A>().to_string();
    tokio::spawn(async move {
        println!("[{}] Message receiver task started.", actor_name_rx);
        while let Some(msg) = rx.recv().await {
            let mut q = queue_rx.lock().await;
            q.push(PrioritizedWrapper(msg));
            notify_rx.notify_one();
        }
        println!(
            "[{}] All senders dropped. Message receiver task terminating.",
            actor_name_rx
        );
    });

    // Processor task
    let actor_name_proc = std::any::type_name::<A>().to_string();
    let tx_clone = tx.clone(); // Clone sender to check if it's closed
    tokio::spawn(async move {
        println!("[{}] Message processor task started.", actor_name_proc);
        loop {
            let msg = loop {
                let mut q = queue.lock().await;
                if let Some(msg) = q.pop() {
                    break msg; // Got a message, break inner loop
                }
                // Queue is empty, check if we should shut down.
                if tx_clone.is_closed() {
                    println!(
                        "[{}] All senders dropped and queue is empty. Processor task terminating.",
                        actor_name_proc
                    );
                    return; // Exit the whole task
                }
                // Release lock and wait for notification
                drop(q);
                notify.notified().await;
            };

            // We have a message, handle it
            if !actor.handle(msg.0).await {
                println!(
                    "[{}] Actor received shutdown signal. Processor task terminating.",
                    actor_name_proc
                );
                break; // Exit outer loop
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
        self.0.priority().cmp(&other.0.priority())
    }
}
