;; SPDX-License-Identifier: GPL-3.0-or-later
;; A log of 2 GiB.
(func (export "init") (param i32 i32) (call $log (i32.const 0) (i32.const 0x7fffffff)))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
