;; SPDX-License-Identifier: GPL-3.0-or-later
;; Params that run past the end of the memory.
(data (i32.const 16) "subscribe")
(func (export "init") (param i32 i32) (drop (call $call (i32.const 16) (i32.const 9) (i32.const 131000) (i32.const 1000))))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
