mod local;

pub use local::Local;

use crate::error::ZlimResult;
use crate::world::World;

#[derive(Default, Debug, Clone, Copy)]
pub struct ScriptFlags {
    pub is_noop: bool,
    pub exclusive: bool,
    pub independent: bool,
    pub main_thread: bool,
}

pub trait Script: Send {
    fn run(&mut self, world: &mut World) -> ZlimResult<()>;

    fn flags(&self) -> ScriptFlags;
}

pub trait WorldScript: Script {}

pub trait EntityScript: Script {}
