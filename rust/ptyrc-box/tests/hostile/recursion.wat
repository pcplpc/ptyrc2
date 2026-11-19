;; SPDX-License-Identifier: GPL-3.0-or-later
;; A recursion without end.
(func $r (param i32) (result i32) (i32.add (call $r (local.get 0)) (i32.const 1)))
(func (export "init") (param i32 i32) (drop (call $r (i32.const 1))))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
