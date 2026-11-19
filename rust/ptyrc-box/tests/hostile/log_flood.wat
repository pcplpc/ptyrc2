;; SPDX-License-Identifier: GPL-3.0-or-later
;; 3 MiB of log, 64 KiB a line.
(func (export "init") (param i32 i32) (call $period (i32.const 100)))
(func (export "tick") (param $now i64) (local $i i32)
  (if (i64.ne (local.get $now) (i64.const 100)) (then (return)))
  (memory.fill (i32.const 65536) (i32.const 0x78) (i32.const 65536))
  (loop $l
    (call $log (i32.const 65536) (i32.const 65536))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br_if $l (i32.lt_u (local.get $i) (i32.const 48)))))
(func (export "event") (param i32 i32 i32 i32))
