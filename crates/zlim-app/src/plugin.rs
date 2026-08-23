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

pub trait Plugin: Any + Send + Sync + 'static {
    /// Initializes the plugin.
    ///
    /// inspects the app, adds dependency plugins
    /// and adjusts the plugin execution order.
    fn build(&self, _app: &mut App) {
        // do nothing
    }

    /// Applies the plugin to the app.
    ///
    /// Called once per plugin, in installation order (dependencies first)
    /// — main-app plugins before every sub-app's plugins.
    fn apply(&self, app: &mut App);

    /// Runs after every plugin has been applied.
    ///
    /// Useful for tearing down temporary resources before the app schedules execute.
    fn cleanup(&mut self, _app: &mut App) {
        // do nothing
    }

    /// A unstable name for debugging.
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

    pub fn is<T: 'static>(&self) -> bool {
        <dyn Any>::type_id(self) == TypeId::of::<T>()
    }
}

// -----------------------------------------------------------------------------
// PlaceholderPlugin

#[derive(Debug, Clone, Copy)]
pub struct PlaceholderPlugin;

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
