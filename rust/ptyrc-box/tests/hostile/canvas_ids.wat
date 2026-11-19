;; SPDX-License-Identifier: GPL-3.0-or-later
;; Ids that name no canvas.
(func (export "init") (param i32 i32) (local $c i32)
  (if (i32.ne (call $present (i32.const 0) (i32.const 0) (i32.const 0)) (i32.const 1)) (then (unreachable)))
  (if (i32.ne (call $present (i32.const -1) (i32.const 0) (i32.const 0)) (i32.const 1)) (then (unreachable)))
  (if (call $cells (i32.const 7)) (then (unreachable)))
  (local.set $c (call $canvas (i32.const 2) (i32.const 2)))
  (call $release (local.get $c))
  (call $release (local.get $c))
  (call $release (i32.const 0))
  (if (call $cells (local.get $c)) (then (unreachable))))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
