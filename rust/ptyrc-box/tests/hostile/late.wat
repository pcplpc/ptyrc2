;; SPDX-License-Identifier: GPL-3.0-or-later
;; Every tick spins for a while.
(func (export "init") (param i32 i32) (call $period (i32.const 100)))
(func (export "tick") (param i64) (local $i i32)
  (loop $l
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br_if $l (i32.lt_u (local.get $i) (i32.const 30000000)))))
(func (export "event") (param i32 i32 i32 i32))
