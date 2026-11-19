;; SPDX-License-Identifier: GPL-3.0-or-later
;; A loop in `tick`.
(func (export "init") (param i32 i32) (call $period (i32.const 100)))
(func (export "tick") (param i64) (loop $l (br $l)))
(func (export "event") (param i32 i32 i32 i32))
