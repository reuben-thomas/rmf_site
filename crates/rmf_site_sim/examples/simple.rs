use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};
use crossbeam_channel::TryRecvError;
use rmf_site_sim::{
    PlaybackUpdate, Simulation, SimulationPlugin, SimulationSet, time::SimulationTime,
};
use std::time::Duration;

fn main() {
    App::new()
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(
                std::time::Duration::from_millis(10),
            )),
            LogPlugin::default(),
            SimulationPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, poll_updates)
        .run();
}

fn poll_updates(mut commands: Commands, simulations: Query<(Entity, &Simulation)>) {
    for (entity, simulation) in &simulations {
        let finished = loop {
            match simulation.playback.try_recv() {
                Ok(PlaybackUpdate::SomeUpdate(time)) => {
                    info!(
                        "Simulation [{}] advanced to {:?}",
                        simulation.name,
                        time.elapsed()
                    );
                }
                Err(TryRecvError::Disconnected) => {
                    info!("Simulation [{}] finished", simulation.name);
                    break true;
                }
                Err(TryRecvError::Empty) => break false,
            }
        };
        if finished {
            commands.entity(entity).despawn();
        }
    }
}

fn setup(mut commands: Commands) {
    let simulation_set = SimulationSet;

    let primary = commands.spawn(simulation_set).id();
    commands.spawn(simulation_set.build_simulation(
        "Primary 0..10".to_string(),
        primary,
        SimulationTime::new(Duration::ZERO),
        SimulationTime::new(Duration::from_secs(10)),
    ));

    let secondary = commands.spawn(simulation_set).id();
    commands.spawn(simulation_set.build_simulation(
        "Secondary 5..15".to_string(),
        secondary,
        SimulationTime::new(Duration::from_secs(5)),
        SimulationTime::new(Duration::from_secs(15)),
    ));
}
