use core::any::Any;
use core::any::TypeId;
use std::collections::BTreeSet;

use zlim_core::schedule::InternedScheduleLabel;
use zlim_core::world::World;
use zlim_log::LogPlugin;
use zlim_task::TaskPoolConfigs;
use zlim_utils::hash::HashMap;

use super::{AppExit, InternedAppLabel};
use super::{DuplicateStrategy, PlaceholderPlugin};
use super::{Plugin, Plugins, PluginsState};

// -----------------------------------------------------------------------------
// RunnerFn

/// The external application loop.
pub type RunnerFn = Box<dyn FnOnce(App) -> AppExit + Send>;
pub type ExtractFn = Box<dyn FnMut(&mut World, &mut World) + Send>;

// -----------------------------------------------------------------------------
// App & SubApp

#[must_use]
pub struct App {
    pub(crate) main: SubApp,
    pub(crate) runner: Option<RunnerFn>,
    pub(crate) sub_apps: HashMap<InternedAppLabel, SubApp>,
    pub(crate) log_plugin: Option<Box<LogPlugin>>,
    pub(crate) task_pool_plugin: Option<Box<TaskPoolConfigs>>,
}

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
            log_plugin: None,
            task_pool_plugin: None,
        }
    }

    #[inline]
    pub fn with_log_plugin(&mut self, plugin: LogPlugin) -> &mut Self {
        assert_eq!(
            self.main.plugins_state,
            PluginsState::Adding,
            "LogPlugin can only be added in `Adding` stage (before `App::build` and `App::run`)."
        );
        self.log_plugin = Some(Box::new(plugin));
        self
    }

    #[inline]
    pub fn config_task_pool(&mut self, configs: TaskPoolConfigs) -> &mut Self {
        assert_eq!(
            self.main.plugins_state,
            PluginsState::Adding,
            "TaskPoolConfigs can only set in `Adding` stage (before `App::build` and `App::run`)."
        );

        self.task_pool_plugin = Some(Box::new(configs));
        self
    }

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
            log_plugin: None,
            task_pool_plugin: None,
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
                s.spawn(async move{ main_world.update_schedules(); });
                for sub_app in app.sub_apps.values_mut() {
                    let world = sub_app.world.as_mut().unwrap();
                    s.spawn(async move{ world.update_schedules(); });
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
        zlim_task::block_on_main(async move { runner(app) })
    }
}

// -----------------------------------------------------------------------------
// Add_plugins

impl App {
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
            log_plugin: None,
            task_pool_plugin: None,
        };

        core::mem::swap(self, &mut app.main);

        f(&mut app);

        core::mem::swap(self, &mut app.main);
        assert!(app.sub_apps.is_empty());
    }

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

            plugin.build(self);

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

        let index = 0usize;
        while index < self.main.plugins.len() {
            let mut plugin: Box<dyn Plugin> = Box::new(PlaceholderPlugin);

            core::mem::swap(&mut plugin, &mut self.main.plugins[index]);

            #[cfg(feature = "trace")]
            let _span = zlim_log::info_span!("plugin cleanup", plugin = plugin.name()).entered();

            plugin.cleanup(self);

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
            // else: There are duplicate plugins inserted, and the old plugins have been replaced.
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
    pub fn build(&mut self) -> &mut Self {
        #[cfg(feature = "trace")]
        let _app_build_span = zlim_log::info_span!("App build").entered();

        match self.main.plugins_state {
            PluginsState::Adding => (),
            PluginsState::Built => panic!("find a nested App::build in `Build` stage"),
            PluginsState::Ready => panic!("find a nested App::build in `Apply` stage"),
            PluginsState::Cleaned => return self,
        }

        // Initialize Log
        if let Some(log_plugin) = self.log_plugin.take() {
            log_plugin.apply();
        }

        // Initialize TaskPool
        if let Some(mut task_pool_plugin) = self.task_pool_plugin.take() {
            task_pool_plugin.apply();
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
    pub fn plugin_mut<T: Plugin>(&mut self) -> Option<&mut T> {
        self.plugins
            .iter_mut()
            .find_map(|x| <dyn Any>::downcast_mut::<T>(&mut **x))
    }

    /// Add a order constraint for Plugin Apply.
    ///
    /// # Panic
    /// Panic if given Plugin is not registered(added).
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
}

impl App {
    /// Check if the plugin has been added.
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
}

// -----------------------------------------------------------------------------
// runner & extract

impl App {
    /// Sets the function that will be called when the app is run.
    #[inline]
    pub fn set_runner(&mut self, f: impl FnOnce(App) -> AppExit + Send + 'static) -> &mut Self {
        self.runner = Some(Box::new(f));
        self
    }
}

impl SubApp {
    /// Sets the method that will be called by [`extract`](Self::extract).
    ///
    /// The first argument is the `World` to extract data from, the second argument is the app `World`.
    pub fn set_extract<F>(&mut self, extract: F) -> &mut Self
    where
        F: FnMut(&mut World, &mut World) + Send + 'static,
    {
        self.extract = Some(Box::new(extract));
        self
    }

    /// Take the function that will be called by [`extract`](Self::extract)
    /// out of the app, if any was set, and replace it with `None`.
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
    pub fn update(&mut self) {
        assert_eq!(self.plugins_state, PluginsState::Cleaned);

        let world: &mut World = match &mut self.world {
            Some(world) => world,
            None => missing_world(),
        };

        if let Some(label) = self.update_schedule {
            world.run_schedule(label);
        }
        world.update_basic();
        world.clear_trackers();
    }
}

impl App {
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
                None => unreachable!("Missing World"),
            };

            if let Some(f) = sub_app.extract.as_mut() {
                f(main_world, world);
            }
            if let Some(label) = sub_app.update_schedule {
                world.run_schedule(label);
            }
            world.update_basic();
            world.clear_trackers();
        }

        main_world.update_basic();
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
