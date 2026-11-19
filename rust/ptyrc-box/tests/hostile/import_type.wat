;; SPDX-License-Identifier: GPL-3.0-or-later
;; `log` of another type.
(import "ptyrc" "log" (func (param i64)))
(memory (export "memory") 1)
(func (export "alloc") (param i32) (result i32) (i32.const 0))
(func (export "free") (param i32 i32))
(func (export "init") (param i32 i32))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
