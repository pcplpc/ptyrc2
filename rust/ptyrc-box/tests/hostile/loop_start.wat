;; SPDX-License-Identifier: GPL-3.0-or-later
;; A loop in the start function.
(func $s (loop $l (br $l)))
(start $s)
(func (export "init") (param i32 i32))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
