use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Notify};
use async_trait::async_trait;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Priority {
    Low,
    Medium,
    High,
}

pub trait Prioritized {
    fn priority(&self) -> Priority {
        Priority::Medium
    }
}

#[async_trait]
pub trait Actor: Send + 'static {
    type Msg: Send + 'static + Prioritized;

    async fn handle(&mut self, msg: Self::Msg);
}

pub fn spawn_actor<A>(mut actor: A) -> mpsc::Sender<A::Msg>
where
    A: Actor,
{
    let (tx, mut rx) = mpsc::channel::<A::Msg>(32);
    let queue = Arc::new(Mutex::new(BinaryHeap::<PrioritizedWrapper<A::Msg>>::new()));
    let notify = Arc::new(Notify::new());

    // Fill the queue
    let queue_rx = Arc::clone(&queue);
    let notify_rx = Arc::clone(&notify);
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let mut q = queue_rx.lock().await;
            q.push(PrioritizedWrapper(msg));
            notify_rx.notify_one();
        }
    });

    // Process messages
    tokio::spawn(async move {
        loop {
            notify.notified().await;
            let msg = {
                let mut q = queue.lock().await;
                q.pop()
            };

            if let Some(PrioritizedWrapper(msg)) = msg {
                actor.handle(msg).await;
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

#[macro_export]
macro_rules! define_actor {
    (
        $actor_name:ident {
            $($field_name:ident : $field_ty:ty),* $(,)?
        }

        impl $msg_name:ident {
            $(
                @priority($prio:ident)
                fn $method:ident ( &mut self $(, $arg_name:ident : $arg_ty:ty )* ) $body:block
            )*
        }
    ) => {
        pub struct $actor_name {
            $(pub $field_name : $field_ty),*
        }

        pub enum $msg_name {
            $(
                $method($($arg_ty),*)
            ),*
        }

        impl $crate::Prioritized for $msg_name {
            fn priority(&self) -> $crate::Priority {
                match self {
                    $(
                        $msg_name::$method(..) => $crate::Priority::$prio,
                    )*
                }
            }
        }

        #[async_trait::async_trait]
        impl $crate::Actor for $actor_name {
            type Msg = $msg_name;

            async fn handle(&mut self, msg: Self::Msg) {
                match msg {
                    $(
                        $msg_name::$method($($arg_name),*) => {
                            self.$method($($arg_name),*).await;
                        }
                    ),*
                }
            }
        }

        impl $actor_name {
            $(
                pub async fn $method(&mut self $(, $arg_name : $arg_ty )* ) $body
            )*
        }
    };
}

/*
define_actor! {
    Counter {
        count: i32
    }

    impl CounterMsg {
        @priority(High)
        fn GetValue(&mut self, tx: tokio::sync::oneshot::Sender<i32>) {
            let _ = tx.send(self.count);
        }

        @priority(Low)
        fn Increment(&mut self) {
            self.count += 1;
        }
    }
}

// then
for _ in 0..100 {
    tx.send(CounterMsg::Increment).await.unwrap();
}

tx.send(CounterMsg::GetValue(resp_tx)).await.unwrap();
 */
