spin-rs
===========

[![Build Status](https://travis-ci.org/mvdnes/spin-rs.svg)](https://travis-ci.org/mvdnes/spin-rs)
[![Crates.io version](https://img.shields.io/crates/v/spin.svg)](https://crates.io/crates/spin)

[Documentation](https://mvdnes.github.io/rust-docs/spin-rs/spin/index.html)

This Rust library implements a simple
[spinlock](https://en.wikipedia.org/wiki/Mutex).

Usage
-----

By default this crate only works on nightly but you can disable the default features
if you want to run on stable. Nightly is more efficient than stable currently.

Also, `Once` is only available on nightly as it is only useful when `const_fn` is available.

Include the following code in your Cargo.toml

```toml
[dependencies.spin]
version = "0.4"
# If you want to run on stable you will need to add the following:
# default-features = false
```

Example
-------

When calling `lock` on a `Mutex` you will get a reference to the data. When this
reference is dropped, the lock will be unlocked.

```rust
extern crate spin;

fn main()
{
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
