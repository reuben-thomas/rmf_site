use bevy::prelude::*;
use rmf_site_sim::{SimulationBuilder, SimulationPlugin};
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

fn setup(world: &mut World) {
    info!("main - Spawning robots...");
    world.spawn(TurtleBot {
        name: "TurtleBot1".to_string(),
    });
    world.spawn(TurtleBot {
        name: "TurtleBot2".to_string(),
    });

    let simulation = SimulationBuilder::new()
        .register_component::<TurtleBot>()
        .add_compute_systems(sim_update)
        .build(world);

    world.spawn(simulation);
}

fn sim_update(query: Query<(Entity, &TurtleBot)>) {
    for (_, bot) in query.iter() {
        info!("compute - Found TurtleBot: {}", bot.name);
    }
}
