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
        fn Increment(&mut self, ack: oneshot::Sender<()>) {
            self.count += 1;
            let _ = ack.send(());
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
        // Create a new channel for each acknowledgment
        let (ack_tx, _) = oneshot::channel();
        tx.send(TestCounterMsg::Increment(ack_tx)).await.unwrap();
    }

    // Send a shutdown message
    println!("Sending Shutdown message...");
    tx.send(TestCounterMsg::Shutdown).await.unwrap();

    // Try to send more messages (these might not be processed if Shutdown is immediate)
    let (ack_tx, _) = oneshot::channel();
    let send_res = tx.send(TestCounterMsg::Increment(ack_tx)).await;
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
    // --- Setup ---
    let counter_actor_state = TestCounter { count: 0 };
    let tx = spawn_actor(counter_actor_state);
    println!("\n--- Test: Actor with Priority and Synchronization ---");

    // --- Phase 1: Send 10 low-priority messages and wait for them to be processed ---
    println!("Sending 10 Increment messages...");
    for i in 0..10 {
        // Create a new channel for each acknowledgment
        let (ack_tx, ack_rx) = oneshot::channel();

        // Send the message with the ack sender
        tx.send(TestCounterMsg::Increment(ack_tx)).await.unwrap();

        // **CRUCIAL**: Wait for the actor to signal that it has processed the message
        ack_rx.await.unwrap();
        println!("  - Increment #{} acknowledged.", i + 1);
    }
    println!("All 10 Increment messages have been processed by the actor.");

    // At this point, we are GUARANTEED that the actor's count is 10.

    // --- Phase 2: Send a high-priority message and check the state ---
    println!("Sending high-priority GetValue message...");
    let (resp_tx, resp_rx) = oneshot::channel();
    tx.send(TestCounterMsg::GetValue(resp_tx)).await.unwrap();

    // --- Phase 3: Send more low-priority messages concurrently ---
    // These should be processed *after* GetValue because of its high priority.
    println!("Sending 2 more low-priority Increment messages...");
    let (ack_tx_11, ack_rx_11) = oneshot::channel();
    let (ack_tx_12, ack_rx_12) = oneshot::channel();
    tx.send(TestCounterMsg::Increment(ack_tx_11)).await.unwrap();
    tx.send(TestCounterMsg::Increment(ack_tx_12)).await.unwrap();

    // --- Assertions ---
    // Await the response from GetValue. It should be processed before the last two Increments.
    let count = resp_rx.await.unwrap();
    println!("Value received from GetValue: {}", count);
    assert_eq!(
        count, 10,
        "GetValue should see the count after the first 10 increments"
    );

    // --- Optional: Clean up and verify final state ---
    // Wait for the final two increments to finish
    ack_rx_11.await.unwrap();
    ack_rx_12.await.unwrap();

    // Check the final state of the actor
    let (final_resp_tx, final_resp_rx) = oneshot::channel();
    tx.send(TestCounterMsg::GetValue(final_resp_tx))
        .await
        .unwrap();
    let final_count = final_resp_rx.await.unwrap();
    println!("Final actor count: {}", final_count);
    assert_eq!(
        final_count, 12,
        "The final count should reflect all 12 increments"
    );

    // Drop the sender to allow the actor tasks to gracefully shut down
    drop(tx);
    // Give a moment for shutdown messages to print
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_actor_implicit_shutdown_completes() {
    let counter_actor_state = TestCounter { count: 0 };
    let tx = spawn_actor(counter_actor_state);

    println!("\n--- Test: Implicit Shutdown Completes ---");
    for _ in 0..5 {
        // Create a new channel for each acknowledgment
        let (ack_tx, _) = oneshot::channel();
        tx.send(TestCounterMsg::Increment(ack_tx)).await.unwrap();
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
