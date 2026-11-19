// SPDX-License-Identifier: GPL-3.0-or-later
//! The host functions as a module, `ptyrc:native`: what `prelude.js`
//! stands on. Numbers in, numbers and texts out.

use std::cell::RefCell;
use std::collections::BTreeMap;

use boa_engine::module::SyntheticModuleInitializer;
use boa_engine::native_function::NativeFunction;
use boa_engine::object::builtins::JsUint8Array;
use boa_engine::{Context, JsNativeError, JsResult, JsString, JsValue, Module};

use crate::{host, width};

/// A canvas: its cells in our memory, as the host laid them.
#[derive(Clone, Copy)]
struct Cv {
    ptr: *mut u8,
    w: i32,
    h: i32,
}

thread_local! {
    static CANVASES: RefCell<BTreeMap<i32, Cv>> = const { RefCell::new(BTreeMap::new()) };
}

type Native = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;

const EXPORTS: &[(&str, Native)] = &[
    ("call", call),
    ("canvas", canvas),
    ("set", set),
    ("fill", fill),
    ("text", text),
    ("clear", clear),
    ("present", present),
    ("release", release),
    ("period", period),
    ("log", log),
    ("random", random),
    ("exit", exit),
];

pub fn module(context: &mut Context) -> Module {
    let names: Vec<JsString> = EXPORTS.iter().map(|(n, _)| JsString::from(*n)).collect();
    Module::synthetic(
        &names,
        SyntheticModuleInitializer::from_copy_closure(|module, context| {
            for (name, f) in EXPORTS {
                let f = NativeFunction::from_fn_ptr(*f).to_js_function(context.realm());
                module.set_export(&JsString::from(*name), f.into())?;
            }
            Ok(())
        }),
        None,
        None,
        context,
    )
}

fn int(args: &[JsValue], i: usize) -> i32 {
    args.get(i)
        .and_then(JsValue::as_number)
        .map_or(0, |n| n as i32)
}

fn uint(args: &[JsValue], i: usize) -> u32 {
    args.get(i)
        .and_then(JsValue::as_number)
        .map_or(0, |n| n as i64 as u32)
}

fn string(args: &[JsValue], i: usize) -> String {
    args.get(i)
        .and_then(JsValue::as_string)
        .map(|s| s.to_std_string_lossy())
        .unwrap_or_default()
}

fn cv(args: &[JsValue]) -> Option<Cv> {
    let id = int(args, 0);
    CANVASES.with(|c| c.borrow().get(&id).copied())
}

/// One cell. Outside the canvas: nothing.
fn put(cv: Cv, x: i32, y: i32, scalar: u32, fg: u32, bg: u32, attrs: u32) {
    if x < 0 || y < 0 || x >= cv.w || y >= cv.h {
        return;
    }
    let mut cell = [0u8; 16];
    cell[0..4].copy_from_slice(&scalar.to_le_bytes());
    cell[4..8].copy_from_slice(&fg.to_be_bytes());
    cell[8..12].copy_from_slice(&bg.to_be_bytes());
    cell[12..14].copy_from_slice(&(attrs as u16).to_le_bytes());
    let at = (y as usize * cv.w as usize + x as usize) * 16;
    // SAFETY: the host gave `w * h * 16` bytes at `ptr`; `x`, `y` are
    // inside.
    unsafe { std::ptr::copy_nonoverlapping(cell.as_ptr(), cv.ptr.add(at), 16) };
}

fn call(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    match host::request(&string(args, 0), &string(args, 1)) {
        Some(answer) => Ok(JsString::from(answer.as_str()).into()),
        None => Err(JsNativeError::range()
            .with_message("call: no memory left for the answer")
            .into()),
    }
}

fn canvas(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let (w, h) = (int(args, 0), int(args, 1));
    if w <= 0 || h <= 0 {
        return Ok(0.into());
    }
    match host::canvas_new(w, h) {
        Some((id, ptr)) => {
            CANVASES.with(|c| c.borrow_mut().insert(id, Cv { ptr, w, h }));
            Ok(id.into())
        }
        None => Ok(0.into()),
    }
}

/// `set(id, x, y, scalar, fg, bg, attrs)`
fn set(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    if let Some(cv) = cv(args) {
        put(
            cv,
            int(args, 1),
            int(args, 2),
            uint(args, 3),
            uint(args, 4),
            uint(args, 5),
            uint(args, 6),
        );
    }
    Ok(JsValue::undefined())
}

/// `fill(id, x, y, w, h, scalar, fg, bg, attrs)`
fn fill(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    if let Some(cv) = cv(args) {
        let (x, y) = (int(args, 1), int(args, 2));
        let (w, h) = (int(args, 3), int(args, 4));
        let (scalar, fg, bg, attrs) = (uint(args, 5), uint(args, 6), uint(args, 7), uint(args, 8));
        let step = if width::of(scalar) == 2 { 2 } else { 1 };
        for yy in y.max(0)..y.saturating_add(h).min(cv.h) {
            let mut xx = x;
            while xx < x.saturating_add(w).min(cv.w) {
                put(cv, xx, yy, scalar, fg, bg, attrs);
                xx += step;
            }
        }
    }
    Ok(JsValue::undefined())
}

/// `text(id, x, y, string, fg, bg, attrs) -> x` after the last glyph.
fn text(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let mut x = int(args, 1);
    if let Some(cv) = cv(args) {
        let y = int(args, 2);
        let (fg, bg, attrs) = (uint(args, 4), uint(args, 5), uint(args, 6));
        for ch in string(args, 3).chars() {
            let w = width::of(ch as u32);
            if w == 0 {
                continue;
            }
            put(cv, x, y, ch as u32, fg, bg, attrs);
            x = x.saturating_add(w);
        }
    }
    Ok(x.into())
}

fn clear(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    if let Some(cv) = cv(args) {
        // SAFETY: the host gave `w * h * 16` bytes at `ptr`.
        unsafe { std::ptr::write_bytes(cv.ptr, 0, cv.w as usize * cv.h as usize * 16) };
    }
    Ok(JsValue::undefined())
}

/// `present(id, place) -> boolean`; `place`: JSON.
fn present(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let shown = cv(args).is_some() && host::canvas_present(int(args, 0), &string(args, 1));
    Ok(shown.into())
}

fn release(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let id = int(args, 0);
    if CANVASES.with(|c| c.borrow_mut().remove(&id)).is_some() {
        host::canvas_release(id);
    }
    Ok(JsValue::undefined())
}

fn period(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    host::set_period(int(args, 0).max(0));
    Ok(JsValue::undefined())
}

fn log(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    host::log_line(&string(args, 0));
    Ok(JsValue::undefined())
}

fn random(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let n = int(args, 0).clamp(0, 65536) as usize;
    let mut bytes = vec![0u8; n];
    host::fill_random(&mut bytes);
    Ok(JsUint8Array::from_iter(bytes, context)?.into())
}

fn exit(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    host::exit_now(int(args, 0))
}
