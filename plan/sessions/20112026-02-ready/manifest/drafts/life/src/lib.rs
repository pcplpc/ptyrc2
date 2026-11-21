// SPDX-License-Identifier: GPL-3.0-or-later
use ptyrc_app::{App, Value, app};
struct Stub;
impl App for Stub {
    fn init(_args: Value) -> Self {
        Stub
    }
}
app!(Stub);
