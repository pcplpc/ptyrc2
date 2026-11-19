;; SPDX-License-Identifier: GPL-3.0-or-later
;; A place outside the memory.
(func (export "init") (param i32 i32) (drop (call $present (call $canvas (i32.const 1) (i32.const 1)) (i32.const 0x7ffffff0) (i32.const 32))))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
