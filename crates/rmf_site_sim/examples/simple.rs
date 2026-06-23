use bevy::prelude::*;
use rmf_site_sim::{
    EndCondition, SimulationBuilder, SimulationPlugin, sync::EntityCloner, time::SimulationTime,
};
use std::time::Duration;

#[derive(Component, Clone, Debug)]
struct TurtleBot {
    name: String,
}

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

fn sim_startup(mut commands: Commands) {
    info!("startup - Spawning robots...");
    commands.spawn(TurtleBot {
        name: "TurtleBot1".to_string(),
    });
    commands.spawn(TurtleBot {
        name: "TurtleBot2".to_string(),
    });
}

fn sim_update(query: Query<(Entity, &TurtleBot)>) {
    info!(
        "compute - Found {} robots in the simulation.",
        query.iter().count()
    );
}

fn setup(world: &mut World) {
    let mut entity_cloner = EntityCloner::default();
    entity_cloner.register::<TurtleBot>();

    let set = SimulationBuilder::new()
        .add_startup_systems(sim_startup)
        .add_compute_systems(sim_update);
    let set_entity = world.spawn_empty().id();

    // Robots are spawned by the sim_startup system inside the sim world.
    let simulation = set.build(
        "Primary 0..10".to_string(),
        set_entity,
        EndCondition::Time(SimulationTime::new(Duration::from_secs(10))),
        &mut entity_cloner,
        world,
    );
    world.spawn(simulation);
}
