;; SPDX-License-Identifier: GPL-3.0-or-later
;; `call` in a loop without end.
(data (i32.const 16) "subscribe")
(func (export "init") (param i32 i32) (call $period (i32.const 100)))
(func (export "tick") (param i64)
  (loop $l (drop (call $call (i32.const 16) (i32.const 9) (i32.const 0) (i32.const 0))) (br $l)))
(func (export "event") (param i32 i32 i32 i32))
