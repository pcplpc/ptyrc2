;; SPDX-License-Identifier: GPL-3.0-or-later
;; A WASI import.
(import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))
(memory (export "memory") 1)
