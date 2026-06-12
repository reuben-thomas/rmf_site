use bevy::prelude::*;

use crate::{ComputeClock, ComputeResults, SimTime, update_compute_result};

/// A trait for Bevy components representing state that should participate in the simulation.
pub trait SimComponent: Component {}

// TODO: Replace with a macro
impl<T: Component> SimComponent for T {}

pub trait RegisterSimComponent {
    fn register_sim_component<Time: SimTime, T: SimComponent + Clone>(&mut self) -> &mut Self;
}

impl RegisterSimComponent for App {
    fn register_sim_component<Time: SimTime, T: SimComponent + Clone>(&mut self) -> &mut Self {
        self.init_resource::<ComputeResults<Time, T>>().add_systems(
            Update,
            update_compute_result::<Time, T>.run_if(resource_changed::<ComputeClock<Time>>),
        )
    }
}
