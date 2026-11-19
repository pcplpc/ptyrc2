// SPDX-License-Identifier: GPL-3.0-or-later
//! The imports of ABI 1, and the only `unsafe` that names them.

#[link(wasm_import_module = "ptyrc")]
unsafe extern "C" {
    fn call(method: *const u8, method_len: usize, params: *const u8, params_len: usize) -> i64;
    fn canvas(w: i32, h: i32) -> i32;
    fn cells(id: i32) -> *mut u8;
    fn present(id: i32, place: *const u8, place_len: usize) -> i32;
    fn release(id: i32);
    fn period(ms: i32);
    fn log(text: *const u8, len: usize);
    pub fn random(dest: *mut u8, len: usize);
    fn exit(code: i32) -> !;
}

/// A request of the protocol; the answer, JSON. `None`: no memory.
pub fn request(method: &str, params: &str) -> Option<String> {
    // SAFETY: both texts live through the call.
    let packed = unsafe { call(method.as_ptr(), method.len(), params.as_ptr(), params.len()) };
    if packed == 0 {
        return None;
    }
    let ptr = (packed as u64 >> 32) as usize as *mut u8;
    let len = (packed as u64 & 0xffff_ffff) as usize;
    // SAFETY: the host wrote `len` bytes where our `alloc(len)` pointed.
    let text = unsafe { std::slice::from_raw_parts(ptr, len) };
    let text = String::from_utf8_lossy(text).into_owned();
    crate::guest_free(ptr, len);
    Some(text)
}

/// A canvas and its cells: `(id, ptr)`. `None`: refused.
pub fn canvas_new(w: i32, h: i32) -> Option<(i32, *mut u8)> {
    // SAFETY: plain integers.
    let id = unsafe { canvas(w, h) };
    if id == 0 {
        return None;
    }
    // SAFETY: plain integers.
    let ptr = unsafe { cells(id) };
    (!ptr.is_null()).then_some((id, ptr))
}

pub fn canvas_present(id: i32, place: &str) -> bool {
    // SAFETY: the text lives through the call.
    unsafe { present(id, place.as_ptr(), place.len()) == 0 }
}

pub fn canvas_release(id: i32) {
    // SAFETY: plain integer.
    unsafe { release(id) }
}

pub fn set_period(ms: i32) {
    // SAFETY: plain integer.
    unsafe { period(ms) }
}

pub fn log_line(text: &str) {
    // SAFETY: the text lives through the call.
    unsafe { log(text.as_ptr(), text.len()) }
}

pub fn fill_random(dest: &mut [u8]) {
    for chunk in dest.chunks_mut(65536) {
        // SAFETY: the chunk is ours.
        unsafe { random(chunk.as_mut_ptr(), chunk.len()) }
    }
}

pub fn exit_now(code: i32) -> ! {
    // SAFETY: plain integer.
    unsafe { exit(code) }
}
