// SPDX-License-Identifier: GPL-3.0-or-later
//! Modules: `"ptyrc"`, and the files of the tree by relative path.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

use boa_engine::module::{ModuleLoader, ModuleRequest, Referrer};
use boa_engine::{Context, JsNativeError, JsResult, Module, Source};

/// The name of the API. No file of the tree answers to it.
pub const API: &str = "ptyrc";
/// The host functions under the API. Reachable until `seal`.
pub const NATIVE: &str = "ptyrc:native";

#[derive(Default)]
pub struct Loader {
    sources: RefCell<BTreeMap<String, String>>,
    parsed: RefCell<BTreeMap<String, Module>>,
    api: RefCell<Option<(Module, Module)>>,
    sealed: Cell<bool>,
}

impl Loader {
    /// A file of the tree. A name that is no plain relative path is
    /// dropped.
    pub fn add(&self, name: &str, text: String) {
        if let Some(name) = normal("", name) {
            self.sources.borrow_mut().insert(name, text);
        }
    }

    pub fn set_api(&self, api: Module, natives: Module) {
        *self.api.borrow_mut() = Some((api, natives));
    }

    /// From now on no module reaches the natives.
    pub fn seal(&self) {
        self.sealed.set(true);
    }

    /// The module of the entry file.
    pub fn entry(&self, main: &str, context: &mut Context) -> JsResult<Module> {
        let name = normal("", main).ok_or_else(|| {
            JsNativeError::error().with_message(format!("`{main}`: not a path inside the app"))
        })?;
        self.file(&name, context)
    }

    fn file(&self, name: &str, context: &mut Context) -> JsResult<Module> {
        if let Some(m) = self.parsed.borrow().get(name) {
            return Ok(m.clone());
        }
        let sources = self.sources.borrow();
        let Some(text) = sources.get(name) else {
            let have: Vec<&str> = sources.keys().map(String::as_str).collect();
            return Err(JsNativeError::error()
                .with_message(format!(
                    "`{name}`: no such file in the app\n  hint: the app holds: {}",
                    if have.is_empty() {
                        "nothing".to_owned()
                    } else {
                        have.join(", ")
                    }
                ))
                .into());
        };
        let source = Source::from_bytes(text.as_bytes()).with_path(Path::new(name));
        let module = Module::parse(source, None, context)
            .map_err(|e| JsNativeError::syntax().with_message(located(name, &e.to_string())))?;
        drop(sources);
        self.parsed
            .borrow_mut()
            .insert(name.to_owned(), module.clone());
        Ok(module)
    }
}

impl ModuleLoader for Loader {
    async fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        request: ModuleRequest,
        context: &RefCell<&mut Context>,
    ) -> JsResult<Module> {
        let spec = request.specifier().to_std_string_escaped();
        if spec == API
            && let Some((api, _)) = &*self.api.borrow()
        {
            return Ok(api.clone());
        }
        if spec == NATIVE
            && !self.sealed.get()
            && let Some((_, natives)) = &*self.api.borrow()
        {
            return Ok(natives.clone());
        }
        if !(spec.starts_with("./") || spec.starts_with("../")) {
            return Err(JsNativeError::error()
                .with_message(format!(
                    "import \"{spec}\": no such module\n  hint: \"ptyrc\", or a file of the app by relative path: \"./{}\"",
                    spec.trim_start_matches('/')
                ))
                .into());
        }
        let from = referrer
            .path()
            .and_then(Path::to_str)
            .map(str::to_owned)
            .unwrap_or_default();
        let dir = from.rsplit_once('/').map_or("", |(dir, _)| dir);
        let Some(name) = normal(dir, &spec) else {
            return Err(JsNativeError::error()
                .with_message(format!(
                    "import \"{spec}\" from {from}: outside the app\n  hint: imports stay inside the tree of the app"
                ))
                .into());
        };
        self.file(&name, &mut context.borrow_mut())
    }
}

/// `path` under `dir`, without `.` and `..`. `None`: it leaves the
/// tree, is absolute, or is empty.
fn normal(dir: &str, path: &str) -> Option<String> {
    if path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return None;
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// `SyntaxError: WHAT at line L, col C` as `name:L:C: WHAT`.
fn located(name: &str, error: &str) -> String {
    let what = error.trim_end_matches(" (native)");
    let what = what.strip_prefix("SyntaxError: ").unwrap_or(what);
    let place = what.rsplit_once(" at line ").and_then(|(what, at)| {
        let (line, col) = at.split_once(", col ")?;
        let (line, col) = (line.parse::<u32>().ok()?, col.trim().parse::<u32>().ok()?);
        Some(format!("{name}:{line}:{col}: {what}"))
    });
    place.unwrap_or_else(|| format!("{name}: {what}"))
}
