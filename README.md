# async-thread

<!-- cargo-sync-readme start -->

This crate provides an API identical to `std::thread`. However, `JoinHandle::join` is an `async
fn`.
```rust
let handle = crate::spawn(|| 5usize);
assert_eq!(handle.join().await.map_err(drop), Ok(5));
```

<!-- cargo-sync-readme end -->
