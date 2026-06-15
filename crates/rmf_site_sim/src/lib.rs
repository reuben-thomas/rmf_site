use bevy::prelude::*;
use std::fmt::Debug;
use std::hash::Hash;

pub use compute::*;
pub use event::*;
pub use world::*;

mod compute;
mod event;
mod world;

pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ComputePlugin);
    }
}

/// A trait for types that can be used as a simulation time.
pub trait SimTime: Ord + Hash + Copy + Send + Sync + Debug + 'static {}

impl<T: Ord + Hash + Copy + Send + Sync + Debug + 'static> SimTime for T {}
