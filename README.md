# 🦀 `priact` 🦀

A lightweight and ergonomic Actor implementation for Rust, built on `tokio`, featuring **explicit message prioritization** via a `BinaryHeap`. This is based on the Actor concept from [Swift](https://www.swift.org).

## ✨ Features

  * **Actor Model Implementation:** Provides a robust foundation for building concurrent, stateful components that safely manage mutable data.
  * **Data Race Prevention:** Ensures serial processing of messages, eliminating data races on the actor's internal state.
  * **Asynchronous Messaging:** Leverages `tokio::mpsc` channels for efficient, non-blocking communication with actors.
  * **Message Prioritization:** Messages can be assigned `Low`, `Medium` (the default), or `High` priority, allowing critical operations to be processed ahead of others.
  * **Ergonomic `define_actor!` Macro:** Simplifies actor definition by automatically generating message enums and `handle` logic, reducing boilerplate.
  * **Built on `tokio`:** Seamlessly integrates with the `tokio` asynchronous runtime.

## 💡 Why `priact`?

While Rust's ownership system makes data races less common, managing mutable state across asynchronous tasks can still be challenging. The Actor Model provides a clear pattern for this, and `priact` offers:

  * **Simplicity:** A focused, opinionated implementation of the core Actor Model, without the overhead of a full framework if you only need the actor primitive.
  * **Safety:** Guarantees that your actor's internal state is accessed by only one task at a time.
  * **Control:** The unique message prioritization feature gives you fine-grained control over message processing order, crucial for real-time systems, performance-sensitive applications, or managing resource contention.
  * **Developer Experience:** The `define_actor!` macro makes defining actors straightforward and enjoyable.

## 🚀 Getting Started

Add `priact` to your `Cargo.toml`:

```toml
[dependencies]
priact = "0.1.0" # Check crates.io for the latest version
tokio = { version = "1", features = ["full"] } # Or specific features you need
async-trait = "0.1" # Required by the Actor trait
```

### Basic Usage

Define your actor and its messages using the `define_actor!` macro:

```rust
use priact::{define_actor, spawn_actor, Actor, Priority};
use tokio::sync::oneshot;

// Define your actor's state and its methods
define_actor! {
    /// A simple counter actor.
    TestCounter {
        count: i32,
    }

    // Define the messages your actor can handle
    impl TestCounterMsg {
        // Methods prefixed with @priority will be translated into messages
        // `self` here refers to the actor's internal state

        // High-priority “read” message
        @priority(High)
        fn GetValue(&mut self, tx: oneshot::Sender<i32>) {
            let _ = tx.send(self.count);
        }

        // Low-priority “write” message
        @priority(Low)
        fn Increment(&mut self, ack: oneshot::Sender<()>) {
            self.count += 1;
            let _ = ack.send(());
        }

        // Medium-priority asynchronous decrement
        @priority(Medium)
        async fn DecrementAsync(&mut self) {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            self.count -= 1;
        }
    }
}

#[tokio::main]
async fn main() {
    // Create actor state
    let counter = TestCounter { count: 0 };

    // Spawn it, getting back a `mpsc::Sender<TestCounterMsg>`
    let tx = spawn_actor(counter);

    // Send some messages...
    let (ack_tx, ack_rx) = oneshot::channel();
    tx.send(TestCounterMsg::Increment(ack_tx)).await.unwrap();
    ack_rx.await.unwrap();

    // Query the current count
    let (resp_tx, resp_rx) = oneshot::channel();
    tx.send(TestCounterMsg::GetValue(resp_tx)).await.unwrap();
    let value = resp_rx.await.unwrap();
    println!("Current count = {}", value);

    // Shut down the actor
    tx.send(TestCounterMsg::Shutdown).await.unwrap();
}
```

## 🔍 Under the Hood

1. **mpsc Receiver Task**  
   Listens on a Tokio mpsc channel and pushes messages into a `BinaryHeap<PrioritizedWrapper>`.
2. **Processor Task**  
   Pops highest-priority message, calls your typed `handle` on the actor, and repeats.
3. **Shutdown**  
   - **Explicit:** A `Shutdown` variant returns `false` from `handle`, tearing down both tasks.  
   - **Implicit:** Dropping all `Sender` handles drains the queue then stops.


## 📚 API Reference

  * `define_actor!`: Macro for defining actors and their messages.
  * `spawn_actor<A>(actor: A) -> mpsc::Sender<A::Msg>`: Spawns an actor into a `tokio` task and returns a sender for its messages.
  * `Actor` trait: Defines the behavior of an actor, requiring a `Msg` type and a `handle` method.
  * `Prioritized` trait: Needs to be implemented by your message enum to specify priority.
  * `Priority` enum: `Low`, `Medium`, `High`.

For detailed API documentation, please refer to [docs.rs](https://www.google.com/search?q=https://docs.rs/priact).

## 🤝 Contributing

Contributions are welcome\! Feel free to open issues or submit pull requests.

## 📄 License

This project is licensed under the [MIT License](https://www.google.com/search?q=LICENSE).
