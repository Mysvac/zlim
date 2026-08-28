use core::time::Duration;
use zlim_os::time::Instant;

use crate::{App, AppExit, Plugin};

// ---------------------------------------------------------------------
// RunMode

/// Determines the method used to run an [`App`].
///
/// It is used in the [`ScheduleRunnerPlugin`].
///
/// [`Schedule`]: zlim_core::schedule::Schedule
#[derive(Copy, Clone, Debug)]
pub enum RunMode {
    /// Indicates that the [`App`]'s schedule should run only once.
    Once,
    /// Indicates that the [`App`]'s schedule should run repeatedly.
    Loop {
        /// The minimum [`Duration`] to wait after a [`Schedule`] has
        /// completed before repeating. A value of [`None`] will not wait.
        ///
        /// [`Schedule`]: zlim_core::schedule::Schedule
        wait: Option<Duration>,
    },
}

impl Default for RunMode {
    fn default() -> Self {
        RunMode::Loop { wait: None }
    }
}

// ---------------------------------------------------------------------
// ScheduleRunnerPlugin

/// Configures an [`App`] to run its [`Schedule`] according to a given
/// [`RunMode`].
///
/// This is typically used for non graphical application.
///
/// This plugin sets the App's Runner. Please avoid using it together with
/// other plugins of the same type, as only one of them will take effect.
///
/// [`Schedule`]: zlim_core::schedule::Schedule
#[derive(Debug, Default)]
pub struct ScheduleRunnerPlugin {
    pub run_mode: RunMode,
}

impl ScheduleRunnerPlugin {
    /// See [`RunMode::Once`].
    pub const fn run_once() -> Self {
        ScheduleRunnerPlugin {
            run_mode: RunMode::Once,
        }
    }

    /// See [`RunMode::Loop`].
    pub const fn run_loop(wait_duration: Duration) -> Self {
        ScheduleRunnerPlugin {
            run_mode: RunMode::Loop {
                wait: Some(wait_duration),
            },
        }
    }
}

impl Plugin for ScheduleRunnerPlugin {
    fn apply(&self, app: &mut App) {
        if app.contains_runner() {
            ::core::hint::cold_path();
            zlim_log::warn!("App's runner is overwritten by `ScheduleRunnerPlugin`.");
        }

        let RunMode::Loop { wait } = self.run_mode else {
            app.set_runner(|mut app| {
                app.update();
                app.should_exit().unwrap_or(AppExit::Success)
            });
            return;
        };

        #[cfg(target_family = "wasm")]
        app.set_runner(wasm_loop(wait));

        #[cfg(not(target_family = "wasm"))]
        let Some(wait) = wait else {
            app.set_runner(|mut app| {
                loop {
                    match tick(&mut app) {
                        None => continue,
                        Some(exit) => return exit,
                    }
                }
            });
            return;
        };

        #[cfg(not(target_family = "wasm"))]
        app.set_runner(move |mut app| {
            loop {
                match tick_with(&mut app, wait) {
                    Ok(None) => continue,
                    Ok(Some(delay)) => std::thread::sleep(delay),
                    Err(exit) => return exit,
                }
            }
        });
    }
}

#[inline]
fn tick(app: &mut App) -> Option<AppExit> {
    app.update();
    app.should_exit()
}

#[inline]
fn tick_with(app: &mut App, wait: Duration) -> Result<Option<Duration>, AppExit> {
    let start_time = Instant::now();

    app.update();

    if let Some(exit) = app.should_exit() {
        return Err(exit);
    };

    let exe_time = start_time.elapsed();

    if exe_time < wait {
        return Ok(Some(wait - exe_time));
    }

    Ok(None)
}

#[cfg(target_family = "wasm")]
fn wasm_loop(wait: Option<Duration>) -> impl FnOnce(App) -> AppExit + Send + Sync + 'static {
    use core::cell::RefCell;
    use js_sys::Function;
    use std::rc::Rc;
    use wasm_bindgen::prelude::{Closure, JsCast, JsValue};

    fn set_timeout(callback: &Closure<dyn FnMut()>, dur: Duration) {
        let window: web_sys::Window = web_sys::window().unwrap();

        let callback: &JsValue = callback.as_ref();
        let handler: &Function = JsCast::unchecked_ref(callback);
        let timeout: i32 = dur.as_millis().try_into().unwrap_or(i32::MAX).max(1);

        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(handler, timeout)
            .expect("Should register `setTimeout`.");
    }

    move |app: App| -> AppExit {
        let app = RefCell::new(app);

        let closure = Rc::new(RefCell::new(None));
        let base_closure = closure.clone();

        let data: Box<dyn FnMut()> = if let Some(wait) = wait {
            Box::new(move || {
                let mut app_borrow = app.borrow_mut();
                match tick_with(&mut *app_borrow, wait) {
                    Ok(delay) => {
                        let callback = closure.borrow();
                        let callback = callback.as_ref().unwrap();
                        set_timeout(callback, delay.unwrap_or(Duration::from_millis(1)))
                    }
                    Err(_exit) => {
                        // explicitly release to prevent circular references
                        *closure.borrow_mut() = None;
                    }
                }
            })
        } else {
            Box::new(move || {
                match tick(&mut *app.borrow_mut()) {
                    None => {
                        let callback = closure.borrow();
                        let callback = callback.as_ref().unwrap();
                        set_timeout(callback, Duration::from_millis(1))
                    }
                    Some(_exit) => {
                        // explicitly release to prevent circular references
                        *closure.borrow_mut() = None;
                    }
                }
            })
        };

        *base_closure.borrow_mut() = Some(Closure::wrap(data));
        set_timeout(
            base_closure.borrow().as_ref().unwrap(),
            Duration::from_millis(1),
        );

        AppExit::Success
    }
}
