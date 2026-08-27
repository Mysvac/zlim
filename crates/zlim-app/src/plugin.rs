use core::any::{Any, TypeId};
use std::boxed::Box;
use std::vec::Vec;

use crate::App;

// -----------------------------------------------------------------------------
// PluginsState

/// Plugins state in the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PluginsState {
    /// Plugins are being added.
    Adding,
    /// Plugins are being built.
    Built,
    /// All added plugins have been built and applied.
    Ready,
    /// `cleanup` has been executed for all plugins.
    Cleaned,
}

// -----------------------------------------------------------------------------
// DuplicateStrategy

/// The behavior when the same plugin type is added twice.
///
/// The default is to log a warning and skip the duplicate.
#[derive(Default, PartialEq, Eq, Debug, Clone, Copy)]
pub enum DuplicateStrategy {
    #[default]
    Skip,
    Cover,
    Panic,
}

// -----------------------------------------------------------------------------
// Plugin

/// A pluggable unit of [`App`] configuration.
///
/// Plugins are **lazy**: [`App::add_plugins`] only stores the plugin  —
/// nothing runs until [`App::build`] (called automatically by [`App::run`])
/// executes every plugin through its three lifecycle stages:
///
/// 1. [`build`](Self::build) — inspect the app, add dependency plugins and
///    adjust the plugin execution order.
/// 2. [`apply`](Self::apply) — apply the plugin, in installation order
///    (dependencies first); main-app plugins run before every sub-app's.
/// 3. [`cleanup`](Self::cleanup) — tear down temporary resources after all
///    plugins have been applied.
///
/// The plugin itself must be `Send + Sync` (the app may be moved to the main
/// thread before initialization).
///
/// [`App::run`]: crate::App::run
/// [`App::build`]: crate::App::build
/// [`App::add_plugins`]: crate::App::add_plugins
pub trait Plugin: Any + Send + Sync + 'static {
    /// Initializes the plugin.
    ///
    /// inspects the app, adds dependency plugins
    /// and adjusts the plugin execution order.
    ///
    /// At this stage, you can insert other plugins into the app.
    fn build(&self, _app: &mut App) {
        // do nothing
    }

    /// Applies the plugin to the app.
    ///
    /// Called once per plugin, in installation order (dependencies first)
    /// — main-app plugins before every sub-app's plugins.
    ///
    /// At this stage, the plugin list has stabilized and new additions are prohibited.
    fn apply(&self, app: &mut App);

    /// Runs after every plugin has been applied.
    ///
    /// Useful for tearing down temporary resources before the app schedules execute.
    ///
    /// At this stage, the plugin list has stabilized and new additions are prohibited.
    fn cleanup(&mut self, _app: &mut App) {
        // do nothing
    }

    /// The plugin's name, used in diagnostics and duplicate detection.
    fn name(&self) -> &'static str {
        core::any::type_name::<Self>()
    }

    /// How duplicate additions of this plugin type are handled.
    fn duplicate_strategy(&self) -> DuplicateStrategy {
        DuplicateStrategy::default()
    }
}

impl dyn Plugin {
    /// Returns the [`TypeId`] of the concrete plugin type.
    pub fn id(&self) -> TypeId {
        <dyn Any>::type_id(self)
    }

    /// Returns `true` if this plugin is of type `T`.
    pub fn is<T: 'static>(&self) -> bool {
        <dyn Any>::type_id(self) == TypeId::of::<T>()
    }
}

// -----------------------------------------------------------------------------
// PlaceholderPlugin

/// An internal sentinel used while swapping plugin objects in and out of the
/// app during `build` / `apply` / `cleanup`.
///
/// It is never meant to be added by users; its
/// [`duplicate_strategy`](Self::duplicate_strategy) is
/// [`DuplicateStrategy::Panic`] so it cannot be inserted as a duplicate.
pub(crate) struct PlaceholderPlugin;

impl Plugin for PlaceholderPlugin {
    fn apply(&self, _: &mut App) {}

    fn duplicate_strategy(&self) -> DuplicateStrategy {
        // Placeholders should not be added.
        DuplicateStrategy::Panic
    }
}

// -----------------------------------------------------------------------------
// PluginGroup

pub trait PluginGroup: Sized {
    /// Returns the plugins of this group.
    fn unpack(self) -> Vec<Box<dyn Plugin>>;

    /// The group name used in diagnostics.
    fn name() -> &'static str {
        core::any::type_name::<Self>()
    }
}

// -----------------------------------------------------------------------------
// Plugins

mod sealed {
    pub trait Sealed<Marker> {}

    pub struct PluginMarker;

    pub struct PluginGroupMarker;

    pub struct PluginsTupleMarker;
}

use sealed::*;

/// Values that can be added to an [`App`] via
/// [`App::add_plugins`](crate::App::add_plugins): an individual [`Plugin`],
/// a [`PluginGroup`], or a tuple of plugin-like values (up to 8 items).
pub trait Plugins<Marker>: Sealed<Marker> {
    fn unpack(self) -> impl Iterator<Item = Box<dyn Plugin>>;
}

// ---------------------------------------------------------------------
// Plugin

impl<P: Plugin> Sealed<PluginMarker> for P {}

impl<P: Plugin> Plugins<PluginMarker> for P {
    fn unpack(self) -> impl Iterator<Item = Box<dyn Plugin>> {
        core::iter::once(Box::new(self) as Box<dyn Plugin>)
    }
}

// ---------------------------------------------------------------------
// PluginGroup

impl<P: PluginGroup> Sealed<PluginGroupMarker> for P {}

impl<P: PluginGroup> Plugins<PluginGroupMarker> for P {
    fn unpack(self) -> impl Iterator<Item = Box<dyn Plugin>> {
        PluginGroup::unpack(self).into_iter()
    }
}

// ---------------------------------------------------------------------
// PluginTuple

macro_rules! impl_plugin_tuple {
    (0: []) => {};
    (1 : [0 : P0 TO ]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        impl<P0, TO> Sealed<(PluginsTupleMarker, TO)> for (P0,)
        where
            P0: Plugins<T0>,
        {}

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 8 items long.")]
        impl<P0, TO> Plugins<(PluginsTupleMarker, TO)> for (P0,)
        where
            P0: Plugins<T0>,
        {
            fn unpack(self) -> impl Iterator<Item = Box<dyn Plugin>> {
                self.0.unpack()
            }
        }
    };
    ($num:literal : [$($index:tt : $p:ident $m:ident),+]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        impl<$($p, $m),*> Sealed<(PluginsTupleMarker, ($($m,)*),)> for ($($p,)*)
        where
            $($p: Plugins<$m>),*
        {}

        #[cfg_attr(docsrs, doc(hidden))]
        impl<$($p, $m),*> Plugins<(PluginsTupleMarker, ($($m,)*),)> for ($($p,)*)
        where
            $($p: Plugins<$m>),*
        {
            fn unpack(self) -> impl Iterator<Item = Box<dyn Plugin>> {
                core::iter::empty::<Box<dyn Plugin>>()
                $(
                    .chain(self.$index.unpack())
                )*
            }
        }
    };
}

zlim_utils::range_invoke2!(impl_plugin_tuple, 8);
