// SPDX-License-Identifier: GPL-3.0-or-later
//! The boundary off `wasm32` (`cargo test`): no host. A canvas is cells
//! in memory, `present` shows nothing, `sprite` is none; `call` is answered by what
//! `native::serve` set, else `native`; `log` goes to stderr; `random`
//! is a fixed sequence a thread; `exit` panics. Same items as `ffi.rs`.

use std::cell::{Cell, RefCell};

type Serve = Box<dyn FnMut(&str, &[u8]) -> Vec<u8>>;

thread_local! {
    static SERVE: RefCell<Option<Serve>> = const { RefCell::new(None) };
    static SEED: Cell<u64> = const { Cell::new(0x9e37_79b9_7f4a_7c15) };
    static CANVASES: Cell<u32> = const { Cell::new(0) };
}

/// No host: never called.
pub extern "C" fn alloc(_len: u32) -> u32 {
    0
}

/// No host: never called.
pub extern "C" fn free(_ptr: u32, _len: u32) {}

pub(crate) fn arg(_ptr: u32, _len: u32) -> Vec<u8> {
    Vec::new()
}

pub(crate) fn serve(f: Serve) {
    SERVE.with(|s| *s.borrow_mut() = Some(f));
}

pub(crate) fn call(method: &str, params: &[u8]) -> Option<Vec<u8>> {
    // The handler is taken out while it runs: it may call again.
    let taken = SERVE.with(|s| s.borrow_mut().take());
    let Some(mut f) = taken else {
        let e = r#"{"error":{"code":"native","message":"no host natively","hint":"native::serve answers calls in a test"}}"#;
        return Some(e.as_bytes().to_vec());
    };
    let answer = f(method, params);
    SERVE.with(|s| {
        s.borrow_mut().get_or_insert(f);
    });
    Some(answer)
}

pub(crate) fn canvas(_w: u32, _h: u32) -> u32 {
    CANVASES.with(|n| {
        n.set(n.get() + 1);
        n.get()
    })
}

pub(crate) fn release(_id: u32) {}

pub(crate) fn present(_id: u32, _place: &[u8]) -> bool {
    true
}

pub(crate) fn sprite(_id: u32) -> Option<u64> {
    None
}

pub(crate) fn period(_ms: u32) {}

pub(crate) fn log(text: &str) {
    eprintln!("{text}")
}

pub(crate) fn random(buf: &mut [u8]) {
    SEED.with(|s| {
        for b in buf {
            let mut x = s.get();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            s.set(x);
            *b = (x >> 24) as u8;
        }
    })
}

pub(crate) fn exit(code: i32) -> ! {
    panic!("exit({code})")
}

/// The cells of a canvas, in memory.
pub(crate) struct Cells(Vec<[u8; 16]>);

impl Cells {
    pub(crate) fn of(_id: u32, n: usize) -> Option<Cells> {
        Some(Cells(vec![[0; 16]; n]))
    }

    pub(crate) fn get(&mut self) -> &mut [[u8; 16]] {
        &mut self.0
    }
}

/// The state of the app: one per module.
pub struct Slot<T>(RefCell<Option<T>>);

// SAFETY: natively no host calls the exports; a test that calls them
// does so from one thread, as the host does.
unsafe impl<T> Sync for Slot<T> {}

impl<T> Slot<T> {
    pub const fn new() -> Slot<T> {
        Slot(RefCell::new(None))
    }

    pub(crate) fn cell(&self) -> &RefCell<Option<T>> {
        &self.0
    }
}

impl<T> Default for Slot<T> {
    fn default() -> Slot<T> {
        Slot::new()
    }
}
