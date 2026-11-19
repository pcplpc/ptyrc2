// SPDX-License-Identifier: GPL-3.0-or-later
//! How a guest ends, when it does not end well.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Not a module the engine takes.
    Module(String),
    /// An import outside `ptyrc`, or one `ptyrc` does not have.
    Import(String),
    /// An export missing, or of another type.
    Export(String),
    /// The module asks more memory than the limit at its start.
    Memory,
    /// `init`, `tick`, `event` or `stop` ran past its bound.
    Time(&'static str),
    /// A pointer or a length outside the guest's memory.
    Pointer(String),
    /// The stack ran out.
    Stack,
    /// Any other trap.
    Trap(String),
    /// `exit(code)`.
    Exit(i32),
    /// The driver is gone.
    Host(String),
}

impl Error {
    /// The name of the error, as logged and tested.
    pub fn name(&self) -> &'static str {
        match self {
            Error::Module(_) => "bad-module",
            Error::Import(_) => "bad-import",
            Error::Export(_) => "bad-export",
            Error::Memory => "memory",
            Error::Time(_) => "time",
            Error::Pointer(_) => "bad-pointer",
            Error::Stack => "stack",
            Error::Trap(_) => "trap",
            Error::Exit(_) => "exit",
            Error::Host(_) => "gone",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = self.name();
        match self {
            Error::Module(m) | Error::Trap(m) | Error::Pointer(m) | Error::Host(m) => {
                write!(f, "{name}: {m}")
            }
            Error::Import(m) => write!(
                f,
                "{name}: {m}: a guest imports from the module `ptyrc` alone"
            ),
            Error::Export(m) => write!(f, "{name}: {m}"),
            Error::Memory => write!(f, "{name}: more than the limit at the start"),
            Error::Time(what) => write!(f, "{name}: {what} ran past its bound"),
            Error::Stack => write!(f, "{name}: the stack ran out"),
            Error::Exit(code) => write!(f, "{name}: {code}"),
        }
    }
}

impl std::error::Error for Error {}
