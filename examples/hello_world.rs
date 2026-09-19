//! ```text
//! cargo run --example hello_world
//! ```

use zlim::prelude::{App, Update, job};

// define a job by `job!` or `#[job_fn]`
job! {
    type: HelloWorld,
    system: || std::println!("Hello World!"),
}

fn main() {
    App::new().build().add_job::<HelloWorld>(Update, ()).run();

    // Basic steps:
    // App::new() : create a default App with `MainSchedulePlugin`.
    // App::init_logger() : initialize Log backend, optional.
    // App::add_plugins() : insert custom app plugins, optional.
    // App::build() : build and apply plugins, optional. (auto called by `run`)
    // Other optional operations:
    // add_job / add_message / init_resource / ...
    // App::run() : run app by internal Runner, default is `run_once`
}
