# Brief

This crate is primarily a dumping ground for personal async snippets that may be used in other ToMT projects.

However should this crate prove useful to others please let us know.

# Usage

### `tomt_async::sync::Mutex`

```rust
use std::sync::Arc;
use tomt_async::sync::Mutex;

async fn main()
{
    let shared_mutex = Arc::new(Mutex::new(0));

    do_something(shared_mutex.clone()).await;
}

async do_something(
    shared_mutex: Arc<Mutex<i32>>
) {
    let lock = shared_mutex.lock().await;
    *lock = *lock + 1;

    // lock is released on drop
}
```
