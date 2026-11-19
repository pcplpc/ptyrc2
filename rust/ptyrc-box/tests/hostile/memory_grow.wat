;; SPDX-License-Identifier: GPL-3.0-or-later
;; Grows the memory until refused, then logs the pages held.
(data (i32.const 16) "pages ")
(func (export "init") (param i32 i32) (call $period (i32.const 100)))
(func (export "tick") (param $now i64) (local $n i32) (local $p i32)
  (if (i64.ne (local.get $now) (i64.const 100)) (then (return)))
  (loop $l (br_if $l (i32.ne (memory.grow (i32.const 1)) (i32.const -1))))
  (local.set $n (memory.size))
  (local.set $p (i32.const 25))
  (loop $d
    (i32.store8 (local.get $p) (i32.add (i32.const 0x30) (i32.rem_u (local.get $n) (i32.const 10))))
    (local.set $p (i32.sub (local.get $p) (i32.const 1)))
    (local.set $n (i32.div_u (local.get $n) (i32.const 10)))
    (br_if $d (local.get $n)))
  (call $log (i32.const 16) (i32.const 10)))
(func (export "event") (param i32 i32 i32 i32))
