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
    let builder = SimulationBuilder::new()
        .register_component::<TurtleBot>()
        .add_startup_systems(sim_startup)
        .add_compute_systems(sim_update);

    let mut primary = builder.build();
    primary.sync_from_world(world);
    primary.run_async();
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
    for (_, bot) in query.iter() {
        info!("compute - Found TurtleBot: {}", bot.name);
    }
}
