#![feature(const_fn)]
extern crate spin;

static INIT: spin::Once<usize> = spin::Once::new();

fn get_cached_val() -> usize {
    *INIT.call_once(expensive_computation)
}

fn expensive_computation() -> usize {
    panic!("Once aborted");
}

fn thread_fun() {
    println!("value: {}", get_cached_val());
    println!("Done.");
}

fn main() {
    let j1 = ::std::thread::spawn(thread_fun);
    let j2 = ::std::thread::spawn(thread_fun);

    println!("Thread 1 panicked: {:?}", j1.join().is_err());
    println!("Thread 2 panicked: {:?}", j2.join().is_err());
}
