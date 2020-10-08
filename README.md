# spin-rs

[![Build Status](https://travis-ci.org/mvdnes/spin-rs.svg)](https://travis-ci.org/mvdnes/spin-rs)
[![Crates.io version](https://img.shields.io/crates/v/spin.svg)](https://crates.io/crates/spin)
[![docs.rs](https://docs.rs/spin/badge.svg)](https://docs.rs/spin/)

Spin-based synchronisation primitives.

This crate implements a variety of simple
[spinlock](https://en.wikipedia.org/wiki/Spinlock)-like primitives with similar
interfaces to those in `std::sync`. Because synchronisation uses spinning, the
primitives are suitable for use in `no_std` environments.

Before deciding to use `spin`, we recommend reading
[this superb blog post](https://matklad.github.io/2020/01/02/spinlocks-considered-harmful.html)
by [@matklad](https://github.com/matklad/) that discusses the pros and cons of
spinlocks. If you have access to `std`, it's likely that the primitives in
`std::sync` will serve you better except in very specific circumstances.

## Features

- `Mutex`, `RwLock` and `Once` equivalents.
- Support for `no_std` environments
- [`lock_api`](https://crates.io/crates/lock_api) compatibility
- Upgradeable `RwLock` guards
- Guards can be sent and shared between threads
- Guard leaking

## Usage

Include the following code in your Cargo.toml

```toml
[dependencies.spin]
version = "0.5"
```

## Example

When calling `lock` on a `Mutex` you will get a guard value that allows
referencing the data. When this guard is dropped, the lock will be unlocked.

```rust
fn main() {
    let mutex   = spin::Mutex::new(0);
    let rw_lock = spin::RwLock::new(0);

    // Modify the data
    {
      let mut data = mutex.lock();
      *data = 2;
      let mut data = rw_lock.write();
      *data = 3;
    }

    // Read the data
    let answer = {
      let data1 = mutex.lock();
      let data2 = rw_lock.read();
      let data3 = rw_lock.read(); // sharing
      (*data1, *data2, *data3)
    };

    println!("Answers are {:?}", answer);
}
```

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

`spin` is distributed under the Apache License, Version 2.0, (See `LICENSE` or
https://www.apache.org/licenses/LICENSE-2.0).
