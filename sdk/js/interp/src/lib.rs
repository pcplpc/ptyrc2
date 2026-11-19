// SPDX-License-Identifier: GPL-3.0-or-later
//! The script interpreter: a guest of ABI 1 (`rust/ptyrc-box/README.md`)
//! that runs the ES modules handed to it in `init`.
//!
//! One thread, one context. The host calls one export at a time.

mod host;
mod loader;
mod native;
mod width;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use boa_engine::builtins::promise::PromiseState;
use boa_engine::context::time::{Clock, JsInstant};
use boa_engine::context::{ContextBuilder, HostHooks};
use boa_engine::object::builtins::{JsFunction, JsPromise};
use boa_engine::{Context, JsError, JsResult, JsString, JsValue, Module, Source, js_string};

use loader::Loader;

/// The member of the `init` JSON that holds the sources: name to text.
const SOURCES: &str = "ptyrc:sources";
/// The member of the `init` JSON that names the entry. Default `app.js`.
const MAIN: &str = "ptyrc:main";

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    /// Session time of the call in progress, milliseconds.
    static NOW: Cell<i64> = const { Cell::new(0) };
    /// The last error of the protocol logged.
    static LAST: RefCell<Option<String>> = const { RefCell::new(None) };
}

struct State {
    context: Context,
    tick: Option<JsFunction>,
    event: Option<JsFunction>,
    stop: Option<JsFunction>,
}

/// `Date` and timers of the engine read session time: no other clock.
struct SessionClock;

impl Clock for SessionClock {
    fn now(&self) -> JsInstant {
        let ms = NOW.get().max(0) as u64;
        JsInstant::new(ms / 1000, (ms % 1000) as u32 * 1_000_000)
    }
    fn system_time_millis(&self) -> i64 {
        NOW.get()
    }
}

struct Hooks;

impl HostHooks for Hooks {
    fn utc_now(&self) -> i64 {
        NOW.get()
    }
    fn local_timezone_offset_seconds(&self, _unix_time_seconds: i64) -> i32 {
        0
    }
    fn max_buffer_size(&self, _context: &mut Context) -> u64 {
        16 << 20
    }
}

/// `Math.random` and the engine's hashes draw from the host.
#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    let mut done = 0;
    while done < len {
        let n = (len - done).min(65536);
        // SAFETY: `dest` holds `len` bytes, by the contract of getrandom.
        unsafe { host::random(dest.add(done), n) };
        done += n;
    }
    Ok(())
}

#[unsafe(export_name = "alloc")]
extern "C" fn guest_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::dangling_mut();
    }
    match std::alloc::Layout::from_size_align(len, 1) {
        // SAFETY: the size is not zero.
        Ok(layout) => unsafe { std::alloc::alloc_zeroed(layout) },
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(export_name = "free")]
extern "C" fn guest_free(ptr: *mut u8, len: usize) {
    if len == 0 || ptr.is_null() {
        return;
    }
    if let Ok(layout) = std::alloc::Layout::from_size_align(len, 1) {
        // SAFETY: `ptr` came from `guest_alloc(len)`.
        unsafe { std::alloc::dealloc(ptr, layout) };
    }
}

/// The bytes of an argument the host wrote.
fn arg<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        return &[];
    }
    // SAFETY: the host wrote `len` bytes at `ptr`, ours during the call.
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

#[unsafe(export_name = "init")]
extern "C" fn guest_init(ptr: *const u8, len: usize) {
    std::panic::set_hook(Box::new(|info| {
        host::log_line(&format!("interpreter: {info}"))
    }));
    let text = arg(ptr, len);
    match boot(text) {
        Ok(state) => STATE.with(|s| *s.borrow_mut() = Some(state)),
        Err(line) => {
            host::log_line(&line);
            host::exit_now(1);
        }
    }
}

fn boot(text: &[u8]) -> Result<State, String> {
    let mut args: serde_json::Value = if text.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_slice(text).map_err(|e| format!("init: not JSON: {e}"))?
    };
    let (sources, main) = match args.as_object_mut() {
        Some(o) => (
            o.remove(SOURCES),
            o.remove(MAIN)
                .and_then(|m| m.as_str().map(str::to_owned))
                .unwrap_or_else(|| "app.js".to_owned()),
        ),
        None => (None, "app.js".to_owned()),
    };
    let loader = Rc::new(Loader::default());
    if let Some(serde_json::Value::Object(map)) = sources {
        for (name, text) in map {
            if let serde_json::Value::String(text) = text {
                loader.add(&name, text);
            }
        }
    }
    let mut context = ContextBuilder::new()
        .clock(Rc::new(SessionClock))
        .host_hooks(Rc::new(Hooks))
        .module_loader(loader.clone())
        .build()
        .map_err(|e| format!("interpreter: {e}"))?;
    let c = &mut context;

    // The API first, while the natives are reachable; then never again.
    let api = Module::parse(Source::from_bytes(include_str!("prelude.js")), None, c)
        .map_err(|e| format!("interpreter: prelude: {e}"))?;
    let natives = native::module(c);
    loader.set_api(api.clone(), natives);
    settle(&api, c).map_err(|e| format!("interpreter: prelude: {}", describe(&e, c)))?;
    loader.seal();

    let entry = loader.entry(&main, c).map_err(|e| describe(&e, c))?;
    settle(&entry, c).map_err(|e| describe(&e, c))?;
    let ns = entry.namespace(c);
    let export = |name: JsString, c: &mut Context| -> Result<Option<JsFunction>, String> {
        let v = ns.get(name.clone(), c).map_err(|e| describe(&e, c))?;
        if v.is_undefined() {
            return Ok(None);
        }
        v.as_object()
            .and_then(JsFunction::from_object)
            .map(Some)
            .ok_or_else(|| {
                format!(
                    "{main}: the export `{}` is not a function\n  hint: export function {}(...) {{ }}",
                    name.to_std_string_escaped(),
                    name.to_std_string_escaped()
                )
            })
    };
    let init = export(js_string!("init"), c)?;
    let tick = export(js_string!("tick"), c)?;
    let event = export(js_string!("event"), c)?;
    let stop = export(js_string!("stop"), c)?;
    if init.is_none() && tick.is_none() && event.is_none() {
        return Err(format!(
            "{main}: exports none of `init`, `tick`, `event`\n  hint: export function init(args) {{ period(50); }} and export function tick(now) {{ }}"
        ));
    }
    if let Some(init) = init {
        let args = JsValue::from_json(&args, c).map_err(|e| describe(&e, c))?;
        run(&init, &[args], c).map_err(|e| describe(&e, c))?;
    }
    Ok(State {
        context,
        tick,
        event,
        stop,
    })
}

/// Load, link, evaluate; the jobs run to the end.
fn settle(module: &Module, c: &mut Context) -> JsResult<()> {
    let promise = module.load_link_evaluate(c);
    c.run_jobs()?;
    match promise.state() {
        PromiseState::Fulfilled(_) => Ok(()),
        PromiseState::Rejected(e) => Err(JsError::from_opaque(e)),
        PromiseState::Pending => Err(boa_engine::JsNativeError::error()
            .with_message("a module awaits what never comes: no promise outlives a call")
            .into()),
    }
}

/// Call an export; the jobs it queued run before it returns. A
/// promise it returns (an `async` export) and that is rejected is its
/// error.
fn run(f: &JsFunction, args: &[JsValue], c: &mut Context) -> JsResult<()> {
    let out = f.call(&JsValue::undefined(), args, c)?;
    c.run_jobs()?;
    if let Some(promise) = out.as_object().and_then(|o| JsPromise::from_object(o).ok())
        && let PromiseState::Rejected(e) = promise.state()
    {
        return Err(JsError::from_opaque(e));
    }
    Ok(())
}

/// Frames of a trace kept in the log.
const FRAMES: usize = 6;

/// An error as lines of the log: `file:line:column: what`, the frames
/// of the app, and the cure when known.
fn describe(e: &JsError, c: &mut Context) -> String {
    let text = e.to_string();
    let mut lines = text.lines().map(|l| l.trim_end_matches(" (native)"));
    let head = lines.next().unwrap_or_default();
    let mut out = match head.rsplit_once(" (") {
        Some((what, at)) if at.ends_with(')') && at.matches(':').count() >= 2 => {
            format!("{}: {what}", &at[..at.len() - 1])
        }
        _ => head.to_owned(),
    };
    let mut rest: Vec<&str> = Vec::new();
    let mut frames = 0;
    for line in lines {
        if !line.starts_with("    at ") {
            rest.push(line);
        } else if !line.contains("(unknown at") && !line.contains("(native)") {
            frames += 1;
            if frames <= FRAMES {
                rest.push(line);
            }
        }
    }
    for line in rest {
        out.push('\n');
        out.push_str(line);
    }
    if !out.contains("\n  hint: ")
        && let Some(hint) = prop(e, "hint", c).or_else(|| cure(&out).map(str::to_owned))
    {
        out.push_str("\n  hint: ");
        out.push_str(&hint);
    }
    out
}

/// A text property of a thrown value.
fn prop(e: &JsError, name: &str, c: &mut Context) -> Option<String> {
    let value = e.clone().into_opaque(c).ok()?;
    let p = value.as_object()?.get(JsString::from(name), c).ok()?;
    p.as_string().map(|s| s.to_std_string_lossy())
}

/// The cure of a known mistake, from the text of the error.
fn cure(text: &str) -> Option<&'static str> {
    const CURES: &[(&str, &str)] = &[
        (
            "setTimeout",
            "no timer: ask a period (period(50)) and count in tick(now)",
        ),
        (
            "setInterval",
            "no timer: ask a period (period(50)) and count in tick(now)",
        ),
        (
            "requestAnimationFrame",
            "no timer: ask a period (period(33)) and draw in tick(now)",
        ),
        ("fetch", "no network: a store app has none"),
        ("XMLHttpRequest", "no network: a store app has none"),
        ("WebSocket", "no network: a store app has none"),
        (
            "require",
            "ES modules only: import { x } from \"./file.js\"",
        ),
        ("process", "no process: arguments come in init(args)"),
        ("console", "no console: import { log } from \"ptyrc\""),
        ("document", "no page: draw on a Canvas from \"ptyrc\""),
        ("window", "no page: draw on a Canvas from \"ptyrc\""),
    ];
    if !text.contains("is not defined") {
        return None;
    }
    CURES
        .iter()
        .find(|(name, _)| text.contains(&format!("{name} is not defined")))
        .map(|(_, cure)| *cure)
}

/// An export of the script, called by the host. An error of the
/// protocol left uncaught is logged and the app goes on; any other
/// ends it.
fn enter(pick: fn(&State) -> Option<JsFunction>, args: impl FnOnce(&mut Context) -> Vec<JsValue>) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(state) = s.as_mut() else { return };
        let Some(f) = pick(state) else { return };
        let c = &mut state.context;
        let args = args(c);
        if let Err(e) = run(&f, &args, c) {
            let soft = prop(&e, "name", c).as_deref() == Some("PtyrcError");
            let line = describe(&e, c);
            if !soft {
                host::log_line(&line);
                host::exit_now(1);
            }
            // The same refusal at every tick is told once.
            if LAST.replace(Some(line.clone())) != Some(line.clone()) {
                host::log_line(&line);
            }
        }
    });
}

#[unsafe(export_name = "tick")]
extern "C" fn guest_tick(now_ms: i64) {
    NOW.set(now_ms);
    enter(|s| s.tick.clone(), |_| vec![JsValue::from(now_ms as f64)]);
}

#[unsafe(export_name = "event")]
extern "C" fn guest_event(tp: *const u8, tl: usize, dp: *const u8, dl: usize) {
    let topic = String::from_utf8_lossy(arg(tp, tl)).into_owned();
    let data: serde_json::Value = serde_json::from_slice(arg(dp, dl)).unwrap_or_default();
    enter(
        |s| s.event.clone(),
        |c| {
            vec![
                JsValue::from(JsString::from(topic.as_str())),
                JsValue::from_json(&data, c).unwrap_or_default(),
            ]
        },
    );
}

#[unsafe(export_name = "stop")]
extern "C" fn guest_stop() {
    enter(|s| s.stop.clone(), |_| Vec::new());
}
