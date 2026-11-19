;; SPDX-License-Identifier: GPL-3.0-or-later
;; `alloc` calls `call`, whose answer the host allocates with `alloc`.
(import "ptyrc" "call" (func $call (param i32 i32 i32 i32) (result i64)))
(memory (export "memory") 1)
(data (i32.const 16) "subscribe")
(func (export "alloc") (param i32) (result i32)
  (drop (call $call (i32.const 16) (i32.const 9) (i32.const 0) (i32.const 0))) (i32.const 1024))
(func (export "free") (param i32 i32))
(func (export "init") (param i32 i32))
(func (export "tick") (param i64))
(func (export "event") (param i32 i32 i32 i32))
