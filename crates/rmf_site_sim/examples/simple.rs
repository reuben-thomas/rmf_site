use bevy::prelude::*;
use rmf_site_sim::{
    EndCondition, Simulation, SimulationPlugin, SimulationSet, time::SimulationTime,
};
use std::time::Duration;

fn main() {
    App::new()
        .add_plugins((
            bevy::log::LogPlugin::default(),
            SimulationPlugin,
            bevy::app::ScheduleRunnerPlugin::run_loop(Duration::from_millis(16)),
        ))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    let set = SimulationSet;
    let set_entity = commands.spawn(set).id();
    commands.spawn::<Simulation>(set.run(
        "Primary 0..10".to_string(),
        set_entity,
        EndCondition::Time(SimulationTime::new(Duration::from_secs(10))),
        Schedule::new(Update),
    ));
}
