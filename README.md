# spin-rs

[![Crates.io version](https://img.shields.io/crates/v/spin.svg)](https://crates.io/crates/spin)
[![docs.rs](https://docs.rs/spin/badge.svg)](https://docs.rs/spin/)
[![Build Status](https://travis-ci.org/mvdnes/spin-rs.svg)](https://travis-ci.org/mvdnes/spin-rs)

Spin-based synchronization primitives.

This crate provides [spin-based](https://en.wikipedia.org/wiki/Spinlock)
versions of the primitives in `std::sync`. Because synchronization is done
through spinning, the primitives are suitable for use in `no_std` environments.

Before deciding to use `spin`, we recommend reading
[this superb blog post](https://matklad.github.io/2020/01/02/spinlocks-considered-harmful.html)
by [@matklad](https://github.com/matklad/) that discusses the pros and cons of
spinlocks. If you have access to `std`, it's likely that the primitives in
`std::sync` will serve you better except in very specific circumstances.

## Features

- `Mutex`, `RwLock` and `Once` equivalents
- Support for `no_std` environments
- [`lock_api`](https://crates.io/crates/lock_api) compatibility
- Upgradeable `RwLock` guards
- Guards can be sent and shared between threads
- Guard leaking
- `std` feature to enable yield to the OS scheduler in busy loops
- `Mutex` can become a ticket lock

## Usage

Include the following under the `[dependencies]` section in your `Cargo.toml` file.

```toml
spin = "x.y"
```

## Example

When calling `lock` on a `Mutex` you will get a guard value that provides access
to the data. When this guard is dropped, the lock will be unlocked.

```rust
extern crate spin;
use std::{sync::Arc, thread};

fn main() {
    let counter = Arc::new(spin::Mutex::new(0));

    let thread = thread::spawn({
        let counter = counter.clone();
        move || {
            for _ in 0..10 {
                *counter.lock() += 1;
            }
        }
    });

    for _ in 0..10 {
        *counter.lock() += 1;
    }

    thread.join().unwrap();

    assert_eq!(*counter.lock(), 20);
}
```

## Feature flags

The crate comes with a few feature flags that you may wish to use.

- `lock_api` enabled support for [`lock_api`](https://crates.io/crates/lock_api)

- `ticket_mutex` uses a ticket lock for the implementation of `Mutex`

- `std` enables support for thread yielding instead of spinning

## Remarks

It is often desirable to have a lock shared between threads. Wrapping the lock in an
`std::sync::Arc` is route through which this might be achieved.

Locks provide zero-overhead access to their data when accessed through a mutable
reference by using their `get_mut` methods.

The behaviour of these lock is similar to their namesakes in `std::sync`. they
differ on the following:

- Locks will not be poisoned in case of failure.
- Threads will not yield to the OS scheduler when encounter a lock that cannot be
accessed. Instead, they will 'spin' in a busy loop until the lock becomes available.

## License

`spin` is distributed under the MIT License, (See `LICENSE`).
