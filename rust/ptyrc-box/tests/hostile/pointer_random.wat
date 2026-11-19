;; SPDX-License-Identifier: GPL-3.0-or-later
;; Random bytes asked outside the memory.
(func (export "init") (param i32 i32) (call $random (i32.const 131070) (i32.const 16)))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
