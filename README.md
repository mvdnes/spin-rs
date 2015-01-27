spinlock-rs
===========

[![Build Status](https://travis-ci.org/mvdnes/spinlock-rs.svg)](https://travis-ci.org/mvdnes/spinlock-rs)

[Documentation](https://mvdnes.github.io/spinlock-rs/)

This Rust library implements a simple
[spinlock](https://en.wikipedia.org/wiki/Mutex).

Build
-----

Just run `cargo build`.

Usage
-----

When calling `lock` on a `Mutex` you will get a reference to the data. When this
reference is dropped, the lock will be unlocked.

```rust
extern crate spin;

fn main()
{
    let mutex   = spin::Mutex::new(0us);
    let rw_lock = spin::RwLock::new(0us);

    // Modify the data
    {
      let mut data = mutex.lock();
      *data = 2;
      let mut data = rw_lock.write();
      *data = 3;
    }

    // Read the data
    let answer =
    {
      let data1 = mutex.lock();
      let data2 = rw_lock.read();
      let data3 = rw_lock.read(); // sharing
      (*data1, *data2, *data3)
    };

    println!("Answers are {:?}", answer);
}
```

To share the lock, an `Arc<Mutex<T>>` may be used.

Remarks
-------

The behaviour of these lock is similar to their namesakes in `std::sync`. they
differ on the following:

 - The lock will not be poisoned in case of failure;
 - The lock can also be used from a plain thread (such as a bare `pthread`).
