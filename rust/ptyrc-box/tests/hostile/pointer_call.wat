;; SPDX-License-Identifier: GPL-3.0-or-later
;; A method outside the memory.
(func (export "init") (param i32 i32) (drop (call $call (i32.const -16) (i32.const 64) (i32.const 0) (i32.const 0))))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
