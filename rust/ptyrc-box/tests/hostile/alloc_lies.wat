;; SPDX-License-Identifier: GPL-3.0-or-later
;; `alloc` answers a pointer outside the memory.
(memory (export "memory") 1)
(func (export "alloc") (param i32) (result i32) (i32.const 0x7fff0000))
(func (export "free") (param i32 i32))
(func (export "init") (param i32 i32))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
