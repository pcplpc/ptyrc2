// SPDX-License-Identifier: GPL-3.0-or-later
//! The boundary: the imports of `ptyrc`, the exports `alloc` and `free`,
//! the guest's memory. All the `unsafe` of the crate is here.
//!
//! An address is a `u32` of linear memory. Allocations are aligned to
//! 16 and keep the length the host gives back to `free`.

use std::alloc::Layout;
use std::cell::RefCell;

mod sys {
    #[link(wasm_import_module = "ptyrc")]
    unsafe extern "C" {
        pub fn call(method: u32, method_len: u32, params: u32, params_len: u32) -> i64;
        pub fn canvas(w: u32, h: u32) -> u32;
        pub fn cells(id: u32) -> u32;
        pub fn present(id: u32, place: u32, place_len: u32) -> u32;
        pub fn sprite(id: u32) -> i64;
        pub fn release(id: u32);
        pub fn period(ms: u32);
        pub fn log(text: u32, len: u32);
        pub fn random(buf: u32, len: u32);
        pub fn exit(code: i32) -> !;
    }
}

const ALIGN: usize = 16;

fn layout(len: u32) -> Option<Layout> {
    Layout::from_size_align(len as usize, ALIGN).ok()
}

/// `len` bytes for the host; 0: none left. `len` 0: a non-null address.
// Exported to the host alone: natively (`cargo test`) not the libc's.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn alloc(len: u32) -> u32 {
    match layout(len) {
        Some(_) if len == 0 => ALIGN as u32,
        // SAFETY: the layout has a non-zero size.
        Some(l) => unsafe { std::alloc::alloc_zeroed(l) as usize as u32 },
        None => 0,
    }
}

/// What `alloc` gave, with its `len`.
// Exported to the host alone: natively (`cargo test`) not the libc's.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn free(ptr: u32, len: u32) {
    if let Some(l) = layout(len).filter(|_| ptr != 0 && len != 0) {
        // SAFETY: the host gives back what `alloc` gave, with its length.
        unsafe { std::alloc::dealloc(ptr as usize as *mut u8, l) }
    }
}

fn addr(bytes: &[u8]) -> (u32, u32) {
    (bytes.as_ptr() as usize as u32, bytes.len() as u32)
}

/// The bytes the host wrote at `(ptr, len)` for the export running.
pub(crate) fn arg(ptr: u32, len: u32) -> Vec<u8> {
    if len == 0 {
        return Vec::new();
    }
    // SAFETY: the host wrote `len` bytes there with `alloc` and frees
    // them after the export returns.
    unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len as usize) }.to_vec()
}

/// The answer of `call`, raw; `None`: the host could not allocate it.
pub(crate) fn call(method: &str, params: &[u8]) -> Option<Vec<u8>> {
    let ((m, ml), (p, pl)) = (addr(method.as_bytes()), addr(params));
    // SAFETY: both texts are live for the call.
    let answer = unsafe { sys::call(m, ml, p, pl) } as u64;
    let (ptr, len) = ((answer >> 32) as u32, answer as u32);
    if answer == 0 {
        return None;
    }
    let bytes = arg(ptr, len);
    free(ptr, len);
    Some(bytes)
}

pub(crate) fn canvas(w: u32, h: u32) -> u32 {
    // SAFETY: integers only.
    unsafe { sys::canvas(w, h) }
}

pub(crate) fn release(id: u32) {
    // SAFETY: integers only; the host frees the cells, not touched after.
    unsafe { sys::release(id) }
}

pub(crate) fn present(id: u32, place: &[u8]) -> bool {
    let (p, pl) = addr(place);
    // SAFETY: the text is live for the call.
    unsafe { sys::present(id, p, pl) == 0 }
}

pub(crate) fn sprite(id: u32) -> Option<u64> {
    // SAFETY: an integer only.
    u64::try_from(unsafe { sys::sprite(id) }).ok()
}

pub(crate) fn period(ms: u32) {
    // SAFETY: an integer only.
    unsafe { sys::period(ms) }
}

pub(crate) fn log(text: &str) {
    let (p, l) = addr(text.as_bytes());
    // SAFETY: the text is live for the call.
    unsafe { sys::log(p, l) }
}

pub(crate) fn random(buf: &mut [u8]) {
    for chunk in buf.chunks_mut(65536) {
        let (p, l) = addr(chunk);
        // SAFETY: the host writes `l` bytes into a buffer of ours.
        unsafe { sys::random(p, l) }
    }
}

pub(crate) fn exit(code: i32) -> ! {
    // SAFETY: does not return.
    unsafe { sys::exit(code) }
}

/// The cells of canvas `id`, `n` of them, from the host's `alloc`.
pub(crate) struct Cells {
    ptr: u32,
    n: usize,
}

impl Cells {
    pub(crate) fn of(id: u32, n: usize) -> Option<Cells> {
        // SAFETY: an integer only.
        let ptr = unsafe { sys::cells(id) };
        (ptr != 0).then_some(Cells { ptr, n })
    }

    pub(crate) fn get(&mut self) -> &mut [[u8; 16]] {
        // SAFETY: the host allocated `n * 16` bytes there, which hold
        // until `release`; `[u8; 16]` needs no alignment; the host reads
        // them only inside `present`, while no borrow is live.
        unsafe { std::slice::from_raw_parts_mut(self.ptr as usize as *mut [u8; 16], self.n) }
    }
}

/// The state of the app: one per module.
pub struct Slot<T>(RefCell<Option<T>>);

// SAFETY: a guest of `wasm32-unknown-unknown` has one thread, and the
// host calls one export at a time.
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
