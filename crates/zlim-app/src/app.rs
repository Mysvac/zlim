use core::any::Any;
use core::any::TypeId;
use core::fmt::{Debug, Formatter};
use std::collections::BTreeSet;

use zlim_core::error::ErrorHandler;
use zlim_core::schedule::InternedScheduleLabel;
use zlim_core::schedule::Schedule;
use zlim_core::schedule::ScheduleLabel;
use zlim_core::world::World;
use zlim_log::LogPlugin;
use zlim_task::TaskPoolConfigs;
use zlim_utils::hash::HashMap;

use crate::AppLabel;
use crate::DuplicateStrategy;
use crate::plugin::PlaceholderPlugin;
use crate::{AppExit, InternedAppLabel};
use crate::{Plugin, Plugins, PluginsState};

// -----------------------------------------------------------------------------
// RunnerFn

/// The external application loop.
pub type RunnerFn = Box<dyn FnOnce(App) -> AppExit + Send>;

/// Copies data from the main world into a sub-app's world, called once per
/// frame before the sub-app's schedule runs.
pub type ExtractFn = Box<dyn FnMut(&mut World, &mut World) + Send>;

// -----------------------------------------------------------------------------
// App & SubApp

/// [`App`] is the primary API for writing user applications.
///
/// # How it works
///
/// An [`App`] owns a **main** [`SubApp`] — a [`World`] with its own plugins
/// and update schedule — plus any number of additional sub-apps, each with
/// its own world.
///
/// Functionality is added as [`Plugin`]s: they are stored lazily by
/// [`App::add_plugins`] and take effect during [`App::build`], which runs
/// every plugin in dependency order (`build` → `apply` → `cleanup`).
///
/// Once built, [`App::run`] hands the app to a runner ([`App::set_runner`];
/// the default runs a single frame).
///
/// A typical game loop calls [`App::update`] repeatedly, which:
///
/// 1. refreshes the main world's metadata and runs its update schedule
///    (see [`MainSchedulePlugin`] for the default schedules);
///
/// 2. for each sub-app, copies data from the main world via its extract
///    step, refreshes metadata and runs the sub-app's update schedule.
///
/// The loop ends when an [`AppExit`] message is raised (check with [`App::should_exit`]).
///
/// [`App`] methods only affect the main sub-app; access other sub-apps with
/// [`get_sub_app`](App::get_sub_app) or [`get_sub_app_mut`](App::get_sub_app_mut).
///
/// # Example
///
/// ```rust
/// use zlim_app::{App, Plugin};
///
/// struct GreetPlugin;
///
/// impl Plugin for GreetPlugin {
///     fn apply(&self, _: &mut App) {
///         // Register schedules, resources, jobs, ...
///     }
/// }
///
/// // `App::new` wires up the main schedules and core engine features.
/// let mut app = App::new();
/// app.add_plugins(GreetPlugin);
///
/// // `App::run` builds all plugins, then drives the runner until an
/// // `AppExit` is raised.  For a real game loop, set a custom runner with
/// // `App::set_runner`.
/// let exit = app.run();
/// assert!(exit.is_success());
/// ```
///
/// # Lifecycle
///
/// 1. **Adding** — plugins are stored via [`App::add_plugins`]; nothing
///    runs yet.
///
/// 2. **build** — [`App::build`] (called automatically by [`App::run`])
///    initializes logging and the task pools, then executes every plugin in
///    dependency order (`build` → `apply` → `cleanup`).
///
/// 3. **run** — [`App::run`] hands the built app to the runner, which
///    drives [`App::update`] until an [`AppExit`] is raised.
///
/// [`MainSchedulePlugin`]: crate::MainSchedulePlugin
#[must_use]
pub struct App {
    pub(crate) main: SubApp,
    pub(crate) runner: Option<RunnerFn>,
    pub(crate) sub_apps: HashMap<InternedAppLabel, SubApp>,
    pub(crate) error_handler: Option<ErrorHandler>,
    pub(crate) task_pool_configs: Option<Box<TaskPoolConfigs>>,
}

/// A self-contained [`World`] with its own plugins, update schedule and
/// extract step.
///
/// The [`App`] itself is a `SubApp` (its "main" sub-app); additional
/// sub-apps are added with [`App::insert_sub_app`] and driven once per
/// frame by [`App::update`]: first the registered extract function
/// ([`SubApp::set_extract`]) copies data from the main world into this
/// sub-app's world, then its update schedule runs.
///
/// This is the pipelining primitive: a render world is typically a sub-app
/// that extracts a snapshot of the main world each frame and renders it
/// without blocking the main world.
///
/// # Example
///
/// ```rust
/// use zlim_app::SubApp;
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Resource, Clone, Copy)]
/// struct Score(u32);
///
/// let mut sub_app = SubApp::new();
///
/// // Every sub-app has its own world:
/// sub_app.world_mut().insert_resource(Score(0));
/// assert_eq!(sub_app.world().get_resource::<Score>().unwrap().0, 0);
///
/// // The extract step copies data from the main world each frame:
/// sub_app.set_extract(|main, sub| {
///     if let Some(score) = main.get_resource::<Score>() {
///         sub.insert_resource(*score);
///     }
/// });
/// ```
///
/// [`World`]: zlim_core::world::World
pub struct SubApp {
    pub(crate) world: Option<Box<World>>,
    pub(crate) plugins: Vec<Box<dyn Plugin>>,
    pub(crate) plugin_names: Vec<(TypeId, &'static str)>,
    pub(crate) plugin_graph: HashMap<TypeId, BTreeSet<TypeId>>,
    pub(crate) plugins_state: PluginsState,
    pub(crate) update_schedule: Option<InternedScheduleLabel>,
    pub(crate) extract: Option<ExtractFn>,
}

// -----------------------------------------------------------------------------
// App Run

impl App {
    /// Creates a new, empty app with no plugins and the default single-frame runner.
    pub fn empty() -> Self {
        Self {
            main: SubApp {
                world: Some(World::alloc()),
                plugins: Vec::new(),
                plugin_names: Vec::new(),
                plugin_graph: HashMap::new(),
                plugins_state: PluginsState::Adding,
                update_schedule: None,
                extract: None,
            },
            runner: None,
            sub_apps: HashMap::new(),
            error_handler: None,
            task_pool_configs: None,
        }
    }
    /// Initializes the global logger with the default [`LogPlugin`].
    ///
    /// Equivalent to calling [`with_logger`](Self::with_logger) with
    /// [`LogPlugin::default()`].
    ///
    /// # Behavior
    ///
    /// - Logging is disabled by default; call this to enable it.
    /// - The logger can only be initialized **once** per process; later calls
    ///   fail and report an `error` to the log output, but do **not** panic.
    /// - Initialization is **immediate**: the global subscriber is installed
    ///   right away, so every later [`App`] operation is visible in the logs.
    ///
    /// Because of that, call it right after [`App::new`] to avoid the odd
    /// problems caused by invisible logs — in particular, when the `trace`
    /// feature is enabled, jobs/schedules created before the global logger is
    /// up would build their spans while the dispatcher is disabled, leaving
    /// those spans permanently inactive.
    pub fn init_logger(&mut self) -> &mut Self {
        LogPlugin::default().apply();
        self
    }

    /// Initializes the global logger with the given [`LogPlugin`] configuration.
    ///
    /// Equivalent to [`LogPlugin::apply`] with the provided plugin.
    ///
    /// # Behavior
    ///
    /// - Logging is disabled by default; call this to enable it.
    /// - The logger can only be initialized **once** per process; only the
    ///   first configuration takes effect, as logging is process-global. Later
    ///   calls fail and report an `error` to the log output, but do **not**
    ///   panic.
    /// - Initialization is **immediate**: the global subscriber is installed
    ///   right away, so every later [`App`] operation is visible in the logs.
    ///
    /// Because of that, call it right after [`App::new`] to avoid the odd
    /// problems caused by invisible logs — in particular, when the `trace`
    /// feature is enabled, jobs/schedules created before the global logger is
    /// up would build their spans while the dispatcher is disabled, leaving
    /// those spans permanently inactive.
    #[inline]
    pub fn with_logger(&mut self, plugin: LogPlugin) -> &mut Self {
        plugin.apply();
        self
    }

    /// Configures the parameters of the global task pool.
    ///
    /// If not set, default parameters will be used.
    ///
    /// The global task pool is shared across the entire process.
    ///
    /// If multiple apps attempt to configure it, only the first configuration
    /// will take effect; subsequent calls will emit a warning and be ignored.
    ///
    /// The applys operation is deferred until the [`App::build`] or [`App::run`].
    ///
    /// # Panics
    ///
    /// Panics if called after the app has entered the `Building` stage.
    #[inline]
    pub fn with_task_pool_configs(&mut self, configs: TaskPoolConfigs) -> &mut Self {
        assert_eq!(
            self.main.plugins_state,
            PluginsState::Adding,
            "TaskPoolConfigs can only set in `Adding` stage (before `App::build` and `App::run`)."
        );

        self.task_pool_configs = Some(Box::new(configs));
        self
    }

    /// Builds the app (idempotent) and runs it with the runner — the default
    /// runner drives a single frame (`update` + `should_exit`).
    ///
    /// In multi-threaded mode the sub-apps' schedule states are initialized
    /// in parallel before the runner starts.
    pub fn run(&mut self) -> AppExit {
        let mut app = App {
            main: SubApp {
                world: None,
                plugins: Vec::new(),
                plugin_names: Vec::new(),
                plugin_graph: HashMap::new(),
                plugins_state: PluginsState::Adding,
                update_schedule: None,
                extract: None,
            },
            runner: None,
            sub_apps: HashMap::new(),
            error_handler: None,
            task_pool_configs: None,
        };

        core::mem::swap(self, &mut app);

        #[cfg(feature = "trace")]
        let _app_run_span = zlim_log::info_span!("app run").entered();

        // Build and apply all plugins.
        app.build();

        assert!(app.main.world.is_some(), "Missing Main-World");

        zlim_task::cfg::multi_thread! {
            zlim_task::MainTaskPool::get().scope(|s| {
                let main_world = app.main.world.as_mut().unwrap();
                s.spawn(async move{ World::update_schedules(main_world) });

                for sub_app in app.sub_apps.values_mut() {
                    let world = sub_app.world.as_mut().unwrap();
                    s.spawn(async move{ World::update_schedules(world) });
                }
            });
        }

        fn run_once(mut app: App) -> AppExit {
            app.update();
            app.should_exit().unwrap_or(AppExit::Success)
        }

        let runner: RunnerFn = app.runner.take().unwrap_or_else(|| Box::new(run_once));

        // Sent to the main thread for execution.
        // If `App` in the main function and annotated as `#[zlim_main]`,
        // this will be executed directly on the current thread.
        zlim_task::invoke_on_main(move || runner(app))
    }
}

// -----------------------------------------------------------------------------
// Add_plugins

impl App {
    /// Adds one or more plugins (a [`Plugin`], a [`PluginGroup`], or a
    /// tuple of them) to the main sub-app.
    ///
    /// Plugins are **lazy**: they are only stored here and take effect when
    /// [`App::build`] runs.  Duplicates are handled per the plugin's [`duplicate_strategy`].
    ///
    /// # Panics
    ///
    /// Panics if called outside the `Adding` stage (after `build`/`run`).
    ///
    /// [`PluginGroup`]: crate::PluginGroup
    /// [`duplicate_strategy`]: Plugin::duplicate_strategy
    pub fn add_plugins<M>(&mut self, plugins: impl Plugins<M>) -> &mut Self {
        #[cfg(feature = "trace")]
        let _app_add_plugins_span = zlim_log::info_span!("app add_plugins").entered();

        assert_eq!(
            self.main.plugins_state,
            PluginsState::Adding,
            "Plugins can only be added in `Adding` stage (before `App::build` and `App::run`)."
        );

        for mut plugin in plugins.unpack() {
            let id = plugin.id();
            let name = plugin.name();

            #[cfg(feature = "trace")]
            let _span = zlim_log::info_span!("app add_plugin", plugin = name).entered();

            if self
                .main
                .plugin_graph
                .try_insert(id, BTreeSet::new())
                .is_err()
            {
                ::core::hint::cold_path();
                #[expect(clippy::print_stderr, reason = "Logger has not been set yet")]
                match plugin.duplicate_strategy() {
                    DuplicateStrategy::Skip => {
                        std::eprintln!(
                            "Find a duplicated Plugin `{name}`, the new one was skipped."
                        );
                        continue;
                    }
                    DuplicateStrategy::Cover => {
                        let ps = &mut self.main.plugins;
                        let x: &mut Box<dyn Plugin> = ps
                            .iter_mut()
                            .find(|x| x.id() == id || x.is::<PlaceholderPlugin>())
                            .unwrap_or_else(|| {
                                panic!("Find duplicated Plugin `{name}`, but missing object.")
                            });
                        core::mem::swap(x, &mut plugin);
                        std::eprintln!(
                            "Find a duplicated Plugin `{name}`, the old one was replaced."
                        );
                    }
                    DuplicateStrategy::Panic => panic!("duplicated plugin `{name}`"),
                }
            } else {
                self.main.plugins.push(plugin);
                self.main.plugin_names.push((id, name));
            }
        }
        self
    }
}

impl SubApp {
    fn app_scope(&mut self, f: impl FnOnce(&mut App)) {
        let mut app = App {
            main: SubApp {
                world: None,
                plugins: Vec::new(),
                plugin_names: Vec::new(),
                plugin_graph: HashMap::new(),
                plugins_state: PluginsState::Adding,
                update_schedule: None,
                extract: None,
            },
            runner: None,
            sub_apps: HashMap::new(),
            error_handler: None,
            task_pool_configs: None,
        };

        core::mem::swap(self, &mut app.main);

        f(&mut app);

        core::mem::swap(self, &mut app.main);
        assert!(app.sub_apps.is_empty());
    }

    /// Adds one or more plugins to this sub-app (lazy; see [`App::add_plugins`]).
    #[inline]
    pub fn add_plugins<M>(&mut self, plugins: impl Plugins<M>) {
        self.app_scope(|app| {
            app.add_plugins(plugins);
        });
    }
}

// -----------------------------------------------------------------------------
// build

impl App {
    fn build_plugins(&mut self) {
        #[cfg(feature = "trace")]
        let _build_span = zlim_log::info_span!("build plugins").entered();

        let mut index = 0usize;
        while index < self.main.plugins.len() {
            let mut plugin: Box<dyn Plugin> = Box::new(PlaceholderPlugin);

            core::mem::swap(&mut plugin, &mut self.main.plugins[index]);

            #[cfg(feature = "trace")]
            let _span = zlim_log::info_span!("plugin build", plugin = plugin.name()).entered();

            plugin.build(self);

            if self.main.plugins[index].is::<PlaceholderPlugin>() {
                core::mem::swap(&mut plugin, &mut self.main.plugins[index]);
                index += 1;
                continue;
            }

            ::core::hint::cold_path();

            if self.main.plugins[index].id() != plugin.id() {
                ::core::hint::cold_path();
                let x = plugin.name();
                let y = self.main.plugins[index].name();
                panic!("The plugin `{x}` has been replaced with a different type `{y}`.");
            }
            // else: There are duplicate plugins inserted, and the old plugins have been replaced.
            index += 1;
        }
        self.main.plugins_state = PluginsState::Built;
    }

    fn apply_plugins(&mut self) {
        #[cfg(feature = "trace")]
        let _apply_span = zlim_log::info_span!("apply plugins").entered();

        while !self.main.plugin_graph.is_empty() {
            let ty: Option<TypeId> = self
                .main
                .plugin_graph
                .iter()
                .find_map(|(x, y)| y.is_empty().then_some(*x));

            fn find_name(app: &App, ty: TypeId) -> &'static str {
                for (xty, name) in &app.main.plugin_names {
                    if *xty == ty {
                        return name;
                    }
                }
                "_unknown_"
            }

            let ty = ty.unwrap_or_else(|| {
                core::hint::cold_path();
                let mut deps_graph: HashMap<&'static str, Vec<&'static str>> = HashMap::new();

                for (k, v) in self.main.plugin_graph.iter() {
                    let key: &'static str = find_name(self, *k);
                    let val: Vec<&'static str> = v.iter().map(|ty| find_name(self, *ty)).collect();
                    deps_graph.insert(key, val);
                }

                panic!("Find circular dependencies: {deps_graph:?}");
            });

            let _ = self.main.plugin_graph.remove(&ty);
            for x in self.main.plugin_graph.values_mut() {
                x.remove(&ty);
            }

            let index = self.main.plugins.iter().position(|x| x.id() == ty);
            let index: usize = index.unwrap_or_else(|| {
                let name = find_name(self, ty);
                panic!("Missing plugin object `{name}`, perhaps removed during App::build.")
            });

            let mut plugin: Box<dyn Plugin> = Box::new(PlaceholderPlugin);

            core::mem::swap(&mut plugin, &mut self.main.plugins[index]);

            #[cfg(feature = "trace")]
            let _span = zlim_log::info_span!("plugin apply", plugin = plugin.name()).entered();

            plugin.apply(self);

            if self.main.plugins[index].is::<PlaceholderPlugin>() {
                core::mem::swap(&mut plugin, &mut self.main.plugins[index]);
                continue;
            }

            ::core::hint::cold_path();

            if self.main.plugins[index].id() != plugin.id() {
                ::core::hint::cold_path();
                let x = plugin.name();
                let y = self.main.plugins[index].name();
                panic!("The plugin `{x}` has been replaced with a different type `{y}`.");
            }
        }

        self.main.plugins_state = PluginsState::Ready;
    }

    fn clean_plugins(&mut self) {
        #[cfg(feature = "trace")]
        let _clean_span = zlim_log::info_span!("cleanup plugins").entered();

        let mut index = 0usize;
        while index < self.main.plugins.len() {
            let mut plugin: Box<dyn Plugin> = Box::new(PlaceholderPlugin);

            core::mem::swap(&mut plugin, &mut self.main.plugins[index]);

            #[cfg(feature = "trace")]
            let _span = zlim_log::info_span!("plugin cleanup", plugin = plugin.name()).entered();

            plugin.cleanup(self);

            if self.main.plugins[index].is::<PlaceholderPlugin>() {
                core::mem::swap(&mut plugin, &mut self.main.plugins[index]);
                index += 1;
                continue;
            }

            ::core::hint::cold_path();

            if self.main.plugins[index].id() != plugin.id() {
                ::core::hint::cold_path();
                let x = plugin.name();
                let y = self.main.plugins[index].name();
                panic!("The plugin `{x}` has been replaced with a different type `{y}`.");
            }
            // else: There are duplicate plugins inserted, and the old plugins have been replaced.
            index += 1;
        }
        // clean data
        let _ = core::mem::take(&mut self.main.plugins);
        let _ = core::mem::take(&mut self.main.plugin_graph);
        self.main.plugins_state = PluginsState::Cleaned;
    }

    fn build_sub_plugins(&mut self) {
        for (_label, sub_app) in self.sub_apps.iter_mut() {
            #[cfg(feature = "trace")]
            let _span = zlim_log::info_span!("sub app build", name = ?_label).entered();

            sub_app.app_scope(|app| {
                app.build();
            });
        }
    }
}

impl App {
    /// Builds the app: initializes logging and the task pools, then executes
    /// every plugin in dependency order (`build` → `apply` → `cleanup`) for
    /// the main sub-app and each sub-app.
    ///
    /// Idempotent: once `cleanup` has run for all plugins (state
    /// [`PluginsState::Cleaned`]) subsequent calls return immediately.
    ///
    /// Called automatically at the start of [`App::run`].
    pub fn build(&mut self) -> &mut Self {
        match self.main.plugins_state {
            PluginsState::Adding => (),
            PluginsState::Built => panic!("find a nested App::build in `Build` stage"),
            PluginsState::Ready => panic!("find a nested App::build in `Apply` stage"),
            PluginsState::Cleaned => return self,
        }

        #[cfg(feature = "trace")]
        let _app_build_span = zlim_log::info_span!(parent: None, "app build").entered();

        // Initialize TaskPool
        if let Some(mut task_pool_configs) = self.task_pool_configs.take() {
            task_pool_configs.apply();
        } else {
            TaskPoolConfigs::default().apply();
        }

        // Collect all types of information:
        // - Reflect Registry (TypeDB)
        // - ECS ComponentDB
        // - ECS ResourceDB
        // - ECS Job Registry
        // - ECS Job Group Registry
        zlim_core::init::core_init();

        self.build_plugins();
        self.apply_plugins();
        self.clean_plugins();
        self.build_sub_plugins();
        self
    }
}

// -----------------------------------------------------------------------------
// contains_plugin

impl SubApp {
    /// Check if the plugin has been added.
    ///
    /// This function can only be used during the plugin build(or apply) phase.
    ///
    /// Cannot be used as the logic of a app runner, as the plugin will be
    /// removed after it is built.
    pub fn contains_plugin<T: Plugin>(&self) -> bool {
        let id = TypeId::of::<T>();
        match self.plugins_state {
            PluginsState::Adding | PluginsState::Built => self.plugin_graph.contains_key(&id),
            PluginsState::Ready | PluginsState::Cleaned => {
                self.plugin_names.iter().find(|(x, _)| *x == id).is_some()
            }
        }
    }

    /// Get a reference to an already inserted component.
    ///
    /// Do not search for the plugin itself while it is working,
    /// and always return None at this time. Because the plugin
    /// has been temporarily removed at this time.
    ///
    /// Note: `contains_plugin` is not affected by this `temporarily remove`.
    ///
    /// This function can only be used during the plugin build(or apply) phase.
    ///
    /// Cannot be used as the logic of a app runner, as the plugin will be
    /// removed after it is built.
    pub fn plugin<T: Plugin>(&self) -> Option<&T> {
        self.plugins
            .iter()
            .find_map(|x| <dyn Any>::downcast_ref::<T>(&**x))
    }

    /// Get a mutable reference to an already inserted component.
    ///
    /// Do not search for the plugin itself while it is working,
    /// and always return None at this time. Because the plugin
    /// has been temporarily removed at this time.
    ///
    /// Note: `contains_plugin` is not affected by this `temporarily remove`.
    ///
    /// This function can only be used during the plugin build(or apply) phase.
    ///
    /// Cannot be used as the logic of a app runner, as the plugin will be
    /// removed after it is built.
    pub fn plugin_mut<T: Plugin>(&mut self) -> Option<&mut T> {
        self.plugins
            .iter_mut()
            .find_map(|x| <dyn Any>::downcast_mut::<T>(&mut **x))
    }

    /// Add a order constraint for Plugin Apply.
    ///
    /// This function can only be called before the [`App::build`]
    /// or in the [`Plugin::build`].
    ///
    /// # Panic
    /// - Panic if given Plugin is not registered(added).
    /// - Panic if the plugin state is not `Adding`.
    pub fn add_plugin_order<Before: Plugin, After: Plugin>(&mut self) {
        if self.plugins_state != PluginsState::Adding {
            panic!(
                "Plugin-Apply order can only be modified in `Adding` stage.\
                In other words, it can only be modified in the 'build' function of the plugin."
            );
        }
        let before_id = TypeId::of::<Before>();
        let after_id = TypeId::of::<After>();

        if !self.plugin_graph.contains_key(&before_id) {
            ::core::hint::cold_path();
            let name = ::core::any::type_name::<Before>();
            panic!("Try add a plugin order but Plugin `{name}` is not registered.");
        }

        match self.plugin_graph.get_mut(&after_id) {
            Some(deps) => {
                deps.insert(before_id);
            }
            None => {
                ::core::hint::cold_path();
                let name = ::core::any::type_name::<After>();
                panic!("Try add a plugin order but Plugin `{name}` is not registered.");
            }
        }
    }

    /// Return all plugin names in this sub app for debugging.
    ///
    /// The plugin name should not be used for identification as it is unstable.
    pub fn plugin_names(&self) -> Vec<&'static str> {
        self.plugin_names.iter().map(|(_, n)| *n).collect()
    }
}

impl App {
    /// Check if the plugin has been added.
    ///
    /// This function can only be used during the plugin build(or apply) phase.
    ///
    /// Cannot be used as the logic of a app runner, as the plugin will be
    /// removed after it is built.
    pub fn contains_plugin<T: Plugin>(&self) -> bool {
        self.main.contains_plugin::<T>()
    }

    /// Get a reference to an already inserted component.
    ///
    /// Do not search for the plugin itself while it is working,
    /// and always return None at this time. Because the plugin
    /// has been temporarily removed at this time.
    ///
    /// Note: `contains_plugin` is not affected by this `temporarily remove`.
    ///
    /// This function can only be used during the plugin build(or apply) phase.
    ///
    /// Cannot be used as the logic of a app runner, as the plugin will be
    /// removed after it is built.
    pub fn plugin<T: Plugin>(&self) -> Option<&T> {
        self.main.plugin::<T>()
    }

    /// Get a mutable reference to an already inserted component.
    ///
    /// Do not search for the plugin itself while it is working,
    /// and always return None at this time. Because the plugin
    /// has been temporarily removed at this time.
    ///
    /// Note: `contains_plugin` is not affected by this `temporarily remove`.
    ///
    /// This function can only be used during the plugin build(or apply) phase.
    ///
    /// Cannot be used as the logic of a app runner, as the plugin will be
    /// removed after it is built.
    pub fn plugin_mut<T: Plugin>(&mut self) -> Option<&mut T> {
        self.main.plugin_mut::<T>()
    }

    /// Add a order constraint for Plugin Apply.
    ///
    /// # Panic
    /// Panic if given Plugin is not registered(added).
    pub fn add_plugin_order<Before: Plugin, After: Plugin>(&mut self) -> &mut Self {
        self.main.add_plugin_order::<Before, After>();
        self
    }

    /// Return all plugin names in this app for debugging.
    ///
    /// This function only return plugin names for the main world.
    ///
    /// The plugin name should not be used for identification as it is unstable.
    pub fn plugin_names(&self) -> Vec<&'static str> {
        self.main.plugin_names.iter().map(|(_, n)| *n).collect()
    }
}

// -----------------------------------------------------------------------------
// runner & extract

impl App {
    /// Sets the function that will be called when the app is run.
    ///
    /// This function will overwrite the old runner.
    ///
    /// If no runner is set, the app will run only a single frame by
    /// default, similar to calling [`App::update`] once.
    #[inline]
    pub fn set_runner(&mut self, f: impl FnOnce(App) -> AppExit + Send + 'static) -> &mut Self {
        self.runner = Some(Box::new(f));
        self
    }

    /// Return `true` if the runner has already been set.
    ///
    /// If the plugin needs to set runner, please check before
    /// adding and output a warning when overwriting.
    #[inline]
    pub fn contains_runner(&self) -> bool {
        self.runner.is_some()
    }
}

impl SubApp {
    /// Sets the method that will be called by [`extract`].
    ///
    /// The first argument is the `World` to extract data from,
    /// the second argument is the app `World`.
    ///
    /// [`extract`]: Self::extract
    pub fn set_extract<F>(&mut self, extract: F) -> &mut Self
    where
        F: FnMut(&mut World, &mut World) + Send + 'static,
    {
        self.extract = Some(Box::new(extract));
        self
    }

    /// Take the function that will be called by [`extract`]
    /// out of the app, if any was set, and replace it with `None`.
    ///
    /// [`extract`]: Self::extract
    pub fn take_extract(&mut self) -> Option<ExtractFn> {
        self.extract.take()
    }
}

// -----------------------------------------------------------------------------
// update

#[cold]
#[inline(never)]
fn missing_world() -> ! {
    panic!("Missing World")
}

impl SubApp {
    /// Extracts data from `world` into the app's world using the registered extract method.
    ///
    /// **Note:** There is no default extract method. Calling `extract` does nothing if
    /// [`set_extract`](Self::set_extract) has not been called.
    pub fn extract(&mut self, world: &mut World) {
        assert_eq!(self.plugins_state, PluginsState::Cleaned);

        let this_world: &mut World = match &mut self.world {
            Some(world) => world,
            None => missing_world(),
        };

        if let Some(f) = self.extract.as_mut() {
            f(world, this_world);
        }
    }

    /// Runs the default schedule and updates internal component trackers.
    ///
    /// # Panics
    ///
    /// Panic if the `update_schedule` if `Some` but schedule does not exist.
    pub fn update(&mut self) {
        assert_eq!(self.plugins_state, PluginsState::Cleaned);

        let world: &mut World = match &mut self.world {
            Some(world) => world,
            None => missing_world(),
        };

        World::refresh_metadata(world);

        if let Some(label) = self.update_schedule {
            world.run_schedule(label); // panic if schedule does not exist.
        }

        world.clear_trackers();
    }
}

impl App {
    /// Drives one frame: refresh metadata and run the main schedule, then
    /// for each sub-app refresh metadata, extract from the main world and
    /// run its schedule.
    ///
    /// # Panics
    ///
    /// Panic if the `update_schedule` if `Some` but schedule does not exist.
    pub fn update(&mut self) {
        assert_eq!(self.main.plugins_state, PluginsState::Cleaned);

        #[cfg(feature = "trace")]
        let _update_span = zlim_log::info_span!("app update").entered();

        let main_world: &mut World = {
            #[cfg(feature = "trace")]
            let _main_update_span = zlim_log::info_span!("main app").entered();
            let world: &mut World = match &mut self.main.world {
                Some(world) => world,
                None => missing_world(),
            };

            World::refresh_metadata(world);

            match self.main.update_schedule {
                Some(label) => world.run_schedule(label),
                None => ::core::hint::cold_path(),
            }

            world
        };

        for (_label, sub_app) in self.sub_apps.iter_mut() {
            #[cfg(feature = "trace")]
            let _sub_app_span = zlim_log::info_span!("sub app", name = ?_label).entered();

            let world: &mut World = match &mut sub_app.world {
                Some(world) => world,
                None => missing_world(),
            };

            if let Some(f) = sub_app.extract.as_mut() {
                f(main_world, world);
            }

            // should we refresh before world::extract ?
            World::refresh_metadata(world);

            if let Some(label) = sub_app.update_schedule {
                world.run_schedule(label);
            }

            world.clear_trackers();
        }

        main_world.clear_trackers();
    }

    /// Attempts to determine if an [`AppExit`] was raised since the last update.
    ///
    /// Will attempt to return the first [`Error`](AppExit::Error) it encounters.
    /// This should be called after every [`update()`](App::update) otherwise you risk
    /// dropping possible [`AppExit`] events.
    pub fn should_exit(&self) -> Option<AppExit> {
        use zlim_core::message::{MessageCursor, MessageQueue};

        let world: &World = match &self.main.world {
            Some(world) => world,
            None => missing_world(),
        };

        let messages = world.get_resource::<MessageQueue<AppExit>>()?;
        if messages.counter() == 0 {
            return None;
        }
        ::core::hint::cold_path();

        let mut cursor = MessageCursor::<AppExit>::with_index(0);
        let mut messages = cursor.read(messages);

        if messages.len() == 0 {
            ::core::hint::cold_path();
            return None; // usually doesn't happen
        }

        let exit = messages
            .find(|exit| exit.is_error())
            .cloned()
            .unwrap_or(AppExit::Success);

        Some(exit)
    }
}

// -----------------------------------------------------------------------------
// world & sub app

#[cold]
#[inline(never)]
fn missing_sub_app(label: InternedAppLabel) -> ! {
    panic!("No sub-app with label '{label:?}' exists.")
}

impl SubApp {
    /// Return a shared reference of [`World`] in this SubApp.
    #[inline(always)]
    pub fn world(&self) -> &World {
        match &self.world {
            Some(world) => world,
            None => missing_world(),
        }
    }

    /// Return a mutable reference of [`World`] in this SubApp.
    #[inline(always)]
    pub fn world_mut(&mut self) -> &mut World {
        match &mut self.world {
            Some(world) => world,
            None => missing_world(),
        }
    }
}

impl App {
    /// Returns a reference to the main [`SubApp`].
    #[inline(always)]
    pub fn main(&self) -> &SubApp {
        &self.main
    }

    /// Returns a mutable reference to the main [`SubApp`].
    #[inline(always)]
    pub fn main_mut(&mut self) -> &mut SubApp {
        &mut self.main
    }

    /// Returns a shared reference to the main [`SubApp`]'s [`World`].
    ///
    /// This is the same as calling [`app.main().world()`].
    ///
    /// [`app.main().world()`]: SubApp::world
    #[inline(always)]
    pub fn main_world(&self) -> &World {
        match &self.main.world {
            Some(world) => world,
            None => missing_world(),
        }
    }

    /// Returns a mutable reference to the main [`SubApp`]'s [`World`].
    ///
    /// This is the same as calling [`app.main_mut().world_mut()`].
    ///
    /// [`app.main_mut().world_mut()`]: SubApp::world_mut
    #[inline(always)]
    pub fn main_world_mut(&mut self) -> &mut World {
        match &mut self.main.world {
            Some(world) => world,
            None => missing_world(),
        }
    }

    /// Returns a reference to the [`SubApp`] with the given label.
    ///
    /// # Panics
    ///
    /// Panics if the [`SubApp`] doesn't exist.
    #[inline]
    pub fn sub_app(&self, label: impl AppLabel) -> &SubApp {
        let label = label.intern();
        self.get_sub_app(label)
            .unwrap_or_else(|| missing_sub_app(label))
    }

    /// Returns a reference to the [`SubApp`] with the given label.
    ///
    /// # Panics
    ///
    /// Panics if the [`SubApp`] doesn't exist.
    #[inline]
    pub fn sub_app_mut(&mut self, label: impl AppLabel) -> &mut SubApp {
        let label = label.intern();
        self.get_sub_app_mut(label)
            .unwrap_or_else(|| missing_sub_app(label))
    }

    /// Returns a reference to the [`SubApp`] with the given label, if it exists.
    #[inline]
    pub fn get_sub_app(&self, label: impl AppLabel) -> Option<&SubApp> {
        self.sub_apps.get(&label.intern())
    }

    /// Returns a mutable reference to the [`SubApp`] with the given label, if it exists.
    #[inline]
    pub fn get_sub_app_mut(&mut self, label: impl AppLabel) -> Option<&mut SubApp> {
        self.sub_apps.get_mut(&label.intern())
    }
}

// -----------------------------------------------------------------------------
// error_handler

impl App {
    /// Sets a default error handler.
    ///
    /// This applies the given handler to all worlds that do not
    /// have an error handler configured.
    ///
    /// This also affects SubApps added later.
    ///
    /// Note: This function only takes effect on the first call.
    /// Once set, the value cannot be overridden.
    pub fn set_error_handler(&mut self, error_handler: ErrorHandler) {
        if self.error_handler.replace(error_handler).is_some() {
            zlim_log::warn!(
                "Cannot replace error handler. The existing handler will continue to be used."
            );
            return;
        }

        self.main_world_mut().try_set_error_handler(error_handler);

        for sub_apps in self.sub_apps.values_mut() {
            sub_apps.world_mut().try_set_error_handler(error_handler);
        }
    }
}

// -----------------------------------------------------------------------------
// insert & remove sub_app

impl App {
    /// Inserts a [`SubApp`] with the given label.
    pub fn insert_sub_app(&mut self, label: impl AppLabel, mut sub_app: SubApp) {
        if let Some(handler) = self.error_handler {
            sub_app.world_mut().try_set_error_handler(handler);
        }
        self.sub_apps.insert(label.intern(), sub_app);
    }

    /// Removes the [`SubApp`] with the given label, if it exists.
    pub fn remove_sub_app(&mut self, label: impl AppLabel) -> Option<SubApp> {
        self.sub_apps.remove(&label.intern())
    }
}

// -----------------------------------------------------------------------------
// New & Default

impl App {
    /// Creates a new [`App`] with some default structure to enable
    /// core engine features.
    ///
    /// This is the preferred constructor for most use cases.
    ///
    /// This does not include an [`AppRunner`] or [`LogPlugin`];
    /// these should be set up manually if needed.
    ///
    /// [`AppRunner`]: RunnerFn
    pub fn new() -> Self {
        use super::main_schedule::Main;
        use super::main_schedule::MainSchedulePlugin;
        use zlim_core::schedule::ScheduleLabel;

        let mut app = App::empty();
        app.main.update_schedule = Some(Main.intern());

        app.add_plugins(MainSchedulePlugin);

        let main_world = app.main.world.as_mut().unwrap();
        main_world.register_message::<AppExit>();

        app
    }
}

impl Default for App {
    /// Creates a new [`App`] with some default structure to enable
    /// core engine features.
    ///
    /// As same as [`App::new`], this is the preferred constructor for
    /// most use cases.
    ///
    /// This does not include an [`AppRunner`] or [`LogPlugin`];
    /// these should be set up manually if needed.
    ///
    /// [`AppRunner`]: RunnerFn
    fn default() -> Self {
        Self::new()
    }
}

impl SubApp {
    /// Returns a default, empty [`SubApp`].
    pub fn new() -> Self {
        Self {
            world: Some(World::alloc()),
            plugins: Vec::new(),
            plugin_names: Vec::new(),
            plugin_graph: HashMap::new(),
            plugins_state: PluginsState::Adding,
            update_schedule: None,
            extract: None,
        }
    }
}

impl Default for SubApp {
    /// Returns a default, empty [`SubApp`].
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// Debug

impl Debug for SubApp {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SubApp")
            .field("plugins", &self.plugin_names)
            .field("update_schedule", &self.update_schedule)
            .finish_non_exhaustive()
    }
}

impl Debug for App {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("App")
            .field("plugins", &self.main.plugin_names)
            .field("update_schedule", &self.main.update_schedule)
            .field("sub_apps", &self.sub_apps)
            .finish()
    }
}

// -----------------------------------------------------------------------------
// Schedule

impl SubApp {
    /// Returns a mutable reference to the schedule with the given label.
    ///
    /// Initializes a new empty schedule if it doesn't exist.
    pub fn schedule_entry(&mut self, label: impl ScheduleLabel) -> &mut Schedule {
        self.world_mut().schedule_entry(label.intern())
    }
}

impl App {
    /// Returns a mutable reference to the schedule with the given label.
    ///
    /// Initializes a new empty schedule if it doesn't exist.
    pub fn schedule_entry(&mut self, label: impl ScheduleLabel) -> &mut Schedule {
        self.main_world_mut().schedule_entry(label.intern())
    }
}
