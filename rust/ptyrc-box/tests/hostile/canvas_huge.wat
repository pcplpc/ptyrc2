;; SPDX-License-Identifier: GPL-3.0-or-later
;; Canvases past the limit: 2^32 cells, 2^62, no cell, one too many.
(data (i32.const 16) "ids 0 0 0 0 0")
(func $put (param $at i32) (param $id i32)
  (i32.store8 (local.get $at) (i32.add (i32.const 0x30) (local.get $id))))
(func (export "init") (param i32 i32)
  (call $put (i32.const 20) (call $canvas (i32.const 65536) (i32.const 65536)))
  (call $put (i32.const 22) (call $canvas (i32.const -1) (i32.const -1)))
  (call $put (i32.const 24) (call $canvas (i32.const 0) (i32.const 5)))
  (call $put (i32.const 26) (call $canvas (i32.const 96) (i32.const 80)))
  (call $put (i32.const 28) (call $canvas (i32.const 1) (i32.const 1)))
  (call $log (i32.const 16) (i32.const 13)))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
