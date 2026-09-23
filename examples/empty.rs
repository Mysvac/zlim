//! ```text
//! cargo run --example empty
//! ```
//!
//! This example creates a empty app and run it once (by default App runner).

use zlim::prelude::App;

fn main() {
    App::empty().run();

    // `App::empty()` : create a empty app (without any plugin).
    // `App::new()` or `App::default()` : create a empty app with `MainSchedulePlugin`
}
