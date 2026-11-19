// SPDX-License-Identifier: GPL-3.0-or-later
//! The engine: wasmtime, and nothing of it outside this file. A guest
//! is loaded, its imports bound to a `Core`, its exports called under a
//! bound of time (epoch interruption: a thread advances the epoch every
//! `EPOCH`; a call past its deadline traps).

use crate::abi::MODULE;
use crate::core::{Core, Host};
use crate::error::Error;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use wasmtime::{
    Caller, Config, Engine, Extern, Linker, Memory, Module, ResourceLimiter, Store, Trap, TypedFunc,
};

const EPOCH: Duration = Duration::from_millis(10);
const STACK: usize = 512 << 10;
const TABLE: usize = 1 << 16;
const RANDOM: usize = 1 << 16;

const IMPORTS: [&str; 9] = [
    "call", "canvas", "cells", "present", "release", "period", "log", "random", "exit",
];

struct Ctx<H: Host> {
    core: Core<H>,
    /// Why a host function stopped the guest.
    fault: Option<Error>,
}

impl<H: Host> ResourceLimiter for Ctx<H> {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.core.limits.memory {
            self.core.meet("memory");
            return Ok(false);
        }
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(desired <= TABLE)
    }

    fn instances(&self) -> usize {
        1
    }

    fn memories(&self) -> usize {
        1
    }
}

/// The engine of the process, its epoch advanced by one thread for
/// ever.
fn shared() -> Result<&'static Engine, Error> {
    static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config.epoch_interruption(true);
            config.max_wasm_stack(STACK);
            // No memfd for the memory images: a module compiled under
            // the OS profile, which refuses memfd_create.
            config.memory_init_cow(false);
            let engine = Engine::new(&config).map_err(|e| format!("{e:#}"))?;
            let ticking = engine.clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(EPOCH);
                    ticking.increment_epoch();
                }
            });
            Ok(engine)
        })
        .as_ref()
        .map_err(|e| Error::Module(e.clone()))
}

/// Compile, or take the module compiled before from the same bytes:
/// the interpreter of script apps is large and the same for all.
fn compile(engine: &Engine, wasm: &[u8]) -> Result<Module, Error> {
    const KEPT: usize = 4;
    const LARGE: usize = 256 << 10;
    static CACHE: Mutex<Vec<(u64, usize, Module)>> = Mutex::new(Vec::new());
    let module_err = |e: wasmtime::Error| Error::Module(format!("{e:#}"));
    if wasm.len() < LARGE {
        return Module::new(engine, wasm).map_err(module_err);
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    wasm.hash(&mut hasher);
    let key = (hasher.finish(), wasm.len());
    // Held while compiling: a second start of the same module waits.
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, _, module)) = cache.iter().find(|(h, l, _)| (*h, *l) == key) {
        return Ok(module.clone());
    }
    let module = Module::new(engine, wasm).map_err(module_err)?;
    if cache.len() == KEPT {
        cache.remove(0);
    }
    cache.push((key.0, key.1, module.clone()));
    Ok(module)
}

/// A loaded guest.
pub struct Guest<H: Host> {
    store: Store<Ctx<H>>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    free: TypedFunc<(i32, i32), ()>,
    init: TypedFunc<(i32, i32), ()>,
    tick: TypedFunc<i64, ()>,
    event: TypedFunc<(i32, i32, i32, i32), ()>,
    stop: Option<TypedFunc<(), ()>>,
}

fn fault<H: Host>(caller: &mut Caller<'_, Ctx<H>>, e: Error) -> wasmtime::Error {
    let text = e.to_string();
    caller.data_mut().fault.get_or_insert(e);
    wasmtime::Error::msg(text)
}

fn memory<H: Host>(caller: &mut Caller<'_, Ctx<H>>) -> wasmtime::Result<Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(m)) => Ok(m),
        _ => Err(fault(caller, Error::Export("memory".into()))),
    }
}

/// `[ptr, ptr + len)` of the guest, checked.
fn range(size: usize, ptr: i32, len: i32) -> Option<std::ops::Range<usize>> {
    let (ptr, len) = (ptr as u32 as usize, len as u32 as usize);
    let end = ptr.checked_add(len)?;
    (end <= size).then_some(ptr..end)
}

fn read<H: Host>(
    caller: &mut Caller<'_, Ctx<H>>,
    what: &str,
    ptr: i32,
    len: i32,
) -> wasmtime::Result<Vec<u8>> {
    let mem = memory(caller)?;
    let cap = caller.data().core.limits.json;
    let data = mem.data(&*caller);
    match range(data.len(), ptr, len) {
        Some(r) if r.len() <= cap => Ok(data[r].to_vec()),
        Some(_) => Err(fault(
            caller,
            Error::Pointer(format!("{what}: longer than {cap} bytes")),
        )),
        None => Err(fault(
            caller,
            Error::Pointer(format!("{what}: {} bytes at {}", len as u32, ptr as u32)),
        )),
    }
}

/// Bytes allocated by the guest for the host: the pointer, or 0.
fn guest_alloc<H: Host>(caller: &mut Caller<'_, Ctx<H>>, len: usize) -> wasmtime::Result<i32> {
    let Ok(len) = i32::try_from(len) else {
        return Ok(0);
    };
    let alloc = match caller.get_export("alloc").and_then(Extern::into_func) {
        Some(f) => f.typed::<i32, i32>(&*caller)?,
        None => return Err(fault(caller, Error::Export("alloc".into()))),
    };
    let ptr = alloc.call(&mut *caller, len)?;
    if ptr == 0 {
        return Ok(0);
    }
    let mem = memory(caller)?;
    if range(mem.data_size(&*caller), ptr, len).is_none() {
        return Err(fault(
            caller,
            Error::Pointer(format!("alloc: {len} bytes at {}", ptr as u32)),
        ));
    }
    Ok(ptr)
}

fn link<H: Host>(linker: &mut Linker<Ctx<H>>) -> wasmtime::Result<()> {
    linker.func_wrap(
        MODULE,
        "call",
        |mut caller: Caller<'_, Ctx<H>>,
         mp: i32,
         ml: i32,
         pp: i32,
         pl: i32|
         -> wasmtime::Result<i64> {
            let method = read(&mut caller, "call: method", mp, ml)?;
            let params = read(&mut caller, "call: params", pp, pl)?;
            let answer = caller.data_mut().core.call(&method, &params);
            let ptr = guest_alloc(&mut caller, answer.len())?;
            if ptr == 0 {
                return Ok(0);
            }
            let mem = memory(&mut caller)?;
            mem.write(&mut caller, ptr as u32 as usize, &answer)?;
            Ok(((ptr as u32 as i64) << 32) | answer.len() as i64)
        },
    )?;
    linker.func_wrap(
        MODULE,
        "canvas",
        |mut caller: Caller<'_, Ctx<H>>, w: i32, h: i32| -> wasmtime::Result<i32> {
            let id = caller.data_mut().core.canvas(w as u32, h as u32);
            if id == 0 {
                return Ok(0);
            }
            let len = caller.data().core.span(id).map_or(0, |s| s.1);
            let ptr = guest_alloc(&mut caller, len)?;
            if ptr == 0 {
                caller.data_mut().core.release(id);
                return Ok(0);
            }
            let mem = memory(&mut caller)?;
            let at = ptr as u32 as usize;
            mem.data_mut(&mut caller)[at..at + len].fill(0);
            caller.data_mut().core.bind(id, ptr as u32);
            Ok(id as i32)
        },
    )?;
    linker.func_wrap(
        MODULE,
        "cells",
        |caller: Caller<'_, Ctx<H>>, id: i32| -> i32 {
            caller.data().core.span(id as u32).map_or(0, |s| s.0 as i32)
        },
    )?;
    linker.func_wrap(
        MODULE,
        "present",
        |mut caller: Caller<'_, Ctx<H>>, id: i32, pp: i32, pl: i32| -> wasmtime::Result<i32> {
            let place = read(&mut caller, "present: place", pp, pl)?;
            let Some((ptr, len)) = caller.data().core.span(id as u32) else {
                return Ok(1);
            };
            let mem = memory(&mut caller)?;
            let (data, ctx) = mem.data_and_store_mut(&mut caller);
            let Some(cells) = data.get(ptr as usize..ptr as usize + len) else {
                return Ok(1);
            };
            Ok(ctx.core.present(id as u32, &place, cells))
        },
    )?;
    linker.func_wrap(
        MODULE,
        "release",
        |mut caller: Caller<'_, Ctx<H>>, id: i32| -> wasmtime::Result<()> {
            let Some((ptr, len)) = caller.data_mut().core.release(id as u32) else {
                return Ok(());
            };
            if let Some(f) = caller.get_export("free").and_then(Extern::into_func) {
                f.typed::<(i32, i32), ()>(&caller)?
                    .call(&mut caller, (ptr as i32, len as i32))?;
            }
            Ok(())
        },
    )?;
    linker.func_wrap(
        MODULE,
        "period",
        |mut caller: Caller<'_, Ctx<H>>, ms: i32| {
            caller.data_mut().core.set_period(ms.max(0) as u32);
        },
    )?;
    linker.func_wrap(
        MODULE,
        "log",
        |mut caller: Caller<'_, Ctx<H>>, ptr: i32, len: i32| -> wasmtime::Result<()> {
            let text = read(&mut caller, "log", ptr, len)?;
            caller.data_mut().core.log(&text);
            Ok(())
        },
    )?;
    linker.func_wrap(
        MODULE,
        "random",
        |mut caller: Caller<'_, Ctx<H>>, ptr: i32, len: i32| -> wasmtime::Result<()> {
            let mem = memory(&mut caller)?;
            let (data, ctx) = mem.data_and_store_mut(&mut caller);
            match range(data.len(), ptr, len) {
                Some(r) if r.len() <= RANDOM => {
                    ctx.core.host.random(&mut data[r]);
                    Ok(())
                }
                _ => Err(fault(
                    &mut caller,
                    Error::Pointer(format!(
                        "random: {} bytes at {} ({RANDOM} at most)",
                        len as u32, ptr as u32
                    )),
                )),
            }
        },
    )?;
    linker.func_wrap(
        MODULE,
        "exit",
        |mut caller: Caller<'_, Ctx<H>>, code: i32| -> wasmtime::Result<()> {
            caller.data_mut().core.set_exit(code);
            Err(fault(&mut caller, Error::Exit(code)))
        },
    )?;
    Ok(())
}

impl<H: Host> Guest<H> {
    /// Compile and instantiate. Nothing of the guest has run when this
    /// returns (a module with a start function is refused).
    pub fn load(wasm: &[u8], core: Core<H>) -> Result<Guest<H>, Error> {
        let module_err = |e: wasmtime::Error| Error::Module(format!("{e:#}"));
        let engine = shared()?;
        let module = compile(engine, wasm)?;
        for import in module.imports() {
            let known = import.module() == MODULE
                && IMPORTS.contains(&import.name())
                && import.ty().func().is_some();
            if !known {
                return Err(Error::Import(format!(
                    "{}.{}",
                    import.module(),
                    import.name()
                )));
            }
        }
        if module.exports().any(|e| e.name() == "_start") {
            return Err(Error::Export("_start: not an export of ABI 1".into()));
        }
        let mut linker = Linker::new(engine);
        link(&mut linker).map_err(module_err)?;
        let mut store = Store::new(engine, Ctx { core, fault: None });
        store.limiter(|ctx| ctx);
        store.epoch_deadline_trap();
        let init_bound = store.data().core.limits.init;
        store.set_epoch_deadline(epochs(init_bound));
        let instance = linker.instantiate(&mut store, &module).map_err(|e| {
            if store.data().core.met().contains(&"memory") {
                Error::Memory
            } else if matches!(e.downcast_ref::<Trap>(), Some(Trap::Interrupt)) {
                Error::Time("start")
            } else {
                // A signature that is not the ABI's is told here.
                Error::Import(format!("{e:#}"))
            }
        })?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| Error::Export("memory".into()))?;
        macro_rules! export {
            ($name:literal) => {
                instance
                    .get_typed_func(&mut store, $name)
                    .map_err(|e| Error::Export(format!("{}: {e:#}", $name)))?
            };
        }
        let stop = match instance.get_func(&mut store, "stop") {
            Some(_) => Some(export!("stop")),
            None => None,
        };
        Ok(Guest {
            alloc: export!("alloc"),
            free: export!("free"),
            init: export!("init"),
            tick: export!("tick"),
            event: export!("event"),
            stop,
            memory,
            store,
        })
    }

    pub fn core(&self) -> &Core<H> {
        &self.store.data().core
    }

    pub fn core_mut(&mut self) -> &mut Core<H> {
        &mut self.store.data_mut().core
    }

    /// Linear memory now, bytes.
    pub fn memory_size(&self) -> usize {
        self.memory.data_size(&self.store)
    }

    pub fn init(&mut self, args: &[u8], bound: Duration) -> Result<(), Error> {
        self.guarded("init", bound, |g| {
            let (ptr, len) = g.put(args)?;
            g.init.call(&mut g.store, (ptr, len))?;
            g.free.call(&mut g.store, (ptr, len))
        })
    }

    pub fn tick(&mut self, now_ms: u64, bound: Duration) -> Result<(), Error> {
        self.guarded("tick", bound, |g| g.tick.call(&mut g.store, now_ms as i64))
    }

    pub fn event(&mut self, topic: &[u8], data: &[u8], bound: Duration) -> Result<(), Error> {
        self.guarded("event", bound, |g| {
            let (tp, tl) = g.put(topic)?;
            let (dp, dl) = g.put(data)?;
            g.event.call(&mut g.store, (tp, tl, dp, dl))?;
            g.free.call(&mut g.store, (tp, tl))?;
            g.free.call(&mut g.store, (dp, dl))
        })
    }

    /// The optional `stop`.
    pub fn stop(&mut self, bound: Duration) -> Result<(), Error> {
        match self.stop.clone() {
            None => Ok(()),
            Some(stop) => self.guarded("stop", bound, |g| stop.call(&mut g.store, ())),
        }
    }

    /// Bytes written into the guest: `(ptr, len)`.
    fn put(&mut self, bytes: &[u8]) -> wasmtime::Result<(i32, i32)> {
        let len = i32::try_from(bytes.len())?;
        let ptr = self.alloc.call(&mut self.store, len)?;
        let size = self.memory.data_size(&self.store);
        match range(size, ptr, len) {
            Some(r) if ptr != 0 || len == 0 => {
                self.memory.data_mut(&mut self.store)[r].copy_from_slice(bytes);
                Ok((ptr, len))
            }
            _ => {
                let e = Error::Pointer(format!("alloc: {len} bytes at {}", ptr as u32));
                let text = e.to_string();
                self.store.data_mut().fault.get_or_insert(e);
                Err(wasmtime::Error::msg(text))
            }
        }
    }

    fn guarded(
        &mut self,
        what: &'static str,
        bound: Duration,
        f: impl FnOnce(&mut Self) -> wasmtime::Result<()>,
    ) -> Result<(), Error> {
        self.store.set_epoch_deadline(epochs(bound));
        let done = f(self);
        let fault = self.store.data_mut().fault.take();
        match (done, fault) {
            (Ok(()), _) => Ok(()),
            (Err(_), Some(fault)) => Err(fault),
            (Err(e), None) => Err(match e.downcast_ref::<Trap>() {
                Some(Trap::Interrupt) => Error::Time(what),
                Some(Trap::StackOverflow) => Error::Stack,
                Some(trap) => Error::Trap(trap.to_string()),
                None => Error::Trap(format!("{e:#}")),
            }),
        }
    }
}

/// Epochs that cover `bound`: one more than its quotient, as the
/// first may be nearly over.
fn epochs(bound: Duration) -> u64 {
    (bound.as_millis() / EPOCH.as_millis()) as u64 + 1
}
