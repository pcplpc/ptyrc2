;; SPDX-License-Identifier: GPL-3.0-or-later
;; 1500 `call` a tick, a tick every 500 ms.
(data (i32.const 16) "subscribe")
(func (export "init") (param i32 i32) (call $period (i32.const 500)))
(func (export "tick") (param i64) (local $i i32)
  (loop $l
    (drop (call $call (i32.const 16) (i32.const 9) (i32.const 0) (i32.const 0)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br_if $l (i32.lt_u (local.get $i) (i32.const 1500)))))
(func (export "event") (param i32 i32 i32 i32))
