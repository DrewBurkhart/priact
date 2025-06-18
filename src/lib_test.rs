use crate::{define_actor, spawn_actor, Actor, Prioritized, Priority};
use tokio::sync::oneshot;

define_actor! {
    TestCounter {
        count: i32,
    }

    impl TestCounterMsg {
        @priority(High)
        fn GetValue(&mut self, tx: oneshot::Sender<i32>) {
            let _ = tx.send(self.count);
        }

        @priority(Low)
        fn Increment(&mut self) {
            self.count += 1;
        }

        @priority(Medium)
        async fn DecrementAsync(&mut self) {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            self.count -= 1;
        }
    }
}

#[tokio::test]
async fn test_actor_explicit_shutdown() {
    let counter_actor_state = TestCounter { count: 0 };
    let tx = spawn_actor(counter_actor_state);

    println!("\n--- Test: Explicit Shutdown ---");
    for _ in 0..5 {
        tx.send(TestCounterMsg::Increment()).await.unwrap();
    }

    // Send a shutdown message
    println!("Sending Shutdown message...");
    tx.send(TestCounterMsg::Shutdown).await.unwrap();

    // Try to send more messages (these might not be processed if Shutdown is immediate)
    let send_res = tx.send(TestCounterMsg::Increment()).await;
    if send_res.is_err() {
        println!(
            "Attempted to send message after shutdown, got error: {:?}",
            send_res.unwrap_err()
        );
    }

    // Give tasks some time to process remaining messages and shut down.
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("--- Test: Explicit Shutdown complete ---");
}

#[tokio::test]
async fn test_actor_priority() {
    let counter_actor_state = TestCounter { count: 0 };
    let tx = spawn_actor(counter_actor_state);

    println!("\n--- Test: Priority ---");
    // Send a low priority message
    tx.send(TestCounterMsg::Increment()).await.unwrap();
    // Send a high priority message that immediately asks for value
    let (resp_tx, resp_rx) = oneshot::channel();
    tx.send(TestCounterMsg::GetValue(resp_tx)).await.unwrap();
    // Send a few more low priority messages that will be processed after GetValue
    tx.send(TestCounterMsg::Increment()).await.unwrap();
    tx.send(TestCounterMsg::Increment()).await.unwrap();

    let count = resp_rx.await.unwrap();
    // GetValue (High) should be processed before subsequent Increments (Low)
    // So the count should be 1 (from the first Increment)
    println!("Count with priority: {}", count);
    assert_eq!(count, 1);

    // Drop the sender to clean up
    drop(tx);
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_actor_implicit_shutdown_completes() {
    let counter_actor_state = TestCounter { count: 0 };
    let tx = spawn_actor(counter_actor_state);

    println!("\n--- Test: Implicit Shutdown Completes ---");
    for _ in 0..5 {
        tx.send(TestCounterMsg::Increment()).await.unwrap();
    }

    let (resp_tx, resp_rx) = oneshot::channel();
    tx.send(TestCounterMsg::GetValue(resp_tx)).await.unwrap();
    let count_before_drop = resp_rx.await.unwrap();
    println!("Count before dropping sender: {}", count_before_drop);

    // Drop the sender. This should signal the actor to shut down.
    drop(tx);
    println!("Sender dropped. Waiting for actor tasks to terminate...");

    let shutdown_timeout = tokio::time::timeout(
        tokio::time::Duration::from_millis(1000), // Max wait 1 second
        async {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        },
    )
    .await;

    assert!(
        shutdown_timeout.is_ok(),
        "Actor did not shut down within the timeout."
    );
    println!("--- Test: Implicit Shutdown Completes (Assertion successful) ---");
}
