use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};
use crossbeam_channel::TryRecvError;
use rmf_site_sim::{
    CreateSimulation, PlaybackUpdate, Simulation, SimulationPlugin, SimulationSet,
    time::SimulationTime,
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
        .add_systems(Startup, setup_and_start_compute_simulation)
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

fn setup_and_start_compute_simulation(
    mut commands: Commands,
    mut compute: EventWriter<CreateSimulation>,
) {
    let primary = commands.spawn(SimulationSet).id();
    compute.write(CreateSimulation {
        name: "Primary 0..10".to_string(),
        set: primary,
        start_time: SimulationTime::new(Duration::ZERO),
        end_time: SimulationTime::new(Duration::from_secs(10)),
    });

    let secondary = commands.spawn(SimulationSet).id();
    compute.write(CreateSimulation {
        name: "Secondary 5..15".to_string(),
        set: secondary,
        start_time: SimulationTime::new(Duration::from_secs(5)),
        end_time: SimulationTime::new(Duration::from_secs(15)),
    });
}
