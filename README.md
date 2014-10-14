spinlock-rs
===========

[![Build Status](https://travis-ci.org/mvdnes/spinlock-rs.svg)](https://travis-ci.org/mvdnes/spinlock-rs)

[Documentation](https://mvdnes.github.io/spinlock-rs/)

This Rust library implements a simple [spinlock](https://en.wikipedia.org/wiki/Spinlock).

Build
-----

Just run `cargo build`.

Usage
-----

When calling `lock` on a `Spinlock` you will get a reference to the data. When this reference is dropped, the lock will be unlocked.

```rust
extern crate spinlock;
use spinlock::Spinlock;

fn main()
{
    let spinlock = Spinlock::new(0u);

    // Modify the data
    {
      let mut data = spinlock.lock();
      *data = 2;
    }

    // Read the data
    let answer =
    {
      let data = spinlock.lock();
      *data
    };

    println!("Answer is {}", answer);
}
```

To share the lock, an `Arc<Spinlock<T>>` may be used.

Remarks
-------

The behaviour of this lock is similar to that of `std::sync::Mutex`. It differs on the following:
- The lock will not be poisoned in case of failure;
- The lock can also be used from a plain thread (such as a bare `pthread`).
