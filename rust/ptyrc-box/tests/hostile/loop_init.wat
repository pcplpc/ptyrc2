;; SPDX-License-Identifier: GPL-3.0-or-later
;; A loop in `init`.
(func (export "init") (param i32 i32) (loop $l (br $l)))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
