use bevy::prelude::*;

mod clock;
mod event;
mod world;

pub use clock::{AddSimulationClock, SimulationClock, SimulationTime, advance_clock};
pub use event::{
    AddDiscreteEvent, DiscreteEvent, DiscreteEventReader, DiscreteEventWriter, DiscreteEvents,
    update_clock,
};
pub use world::SimulationWorld;

pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(initialize_simulation);
    }
}

#[derive(Event, Default)]
pub struct InitializeSimulation;

fn initialize_simulation(_trigger: Trigger<InitializeSimulation>, mut _commands: Commands) {
    debug!("Initializing simulation");
}
