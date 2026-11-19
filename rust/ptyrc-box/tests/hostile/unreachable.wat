;; SPDX-License-Identifier: GPL-3.0-or-later
;; A trap.
(func (export "init") (param i32 i32) (unreachable))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
