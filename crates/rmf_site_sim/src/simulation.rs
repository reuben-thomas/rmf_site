use crate::time::SimulationTime;
use bevy::{prelude::*, tasks::AsyncComputeTaskPool};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::time::Duration;

/// Plugin for running discrete event simulations.
pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, _app: &mut App) {}
}

/// A set of entities, components, systems, and resources in a [`World`] that can be used to compute a simulation.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SimulationSet;

impl SimulationSet {
    /// Create a [`Simulation`] from this set.
    pub fn build_simulation(
        &self,
        name: String,
        set: Entity,
        start_time: SimulationTime,
        end_time: SimulationTime,
    ) -> Simulation {
        let (sender, receiver) = unbounded();
        AsyncComputeTaskPool::get()
            .spawn(async move {
                Simulation::run(sender, start_time, end_time);
            })
            .detach();

        Simulation {
            name,
            set,
            world: World::new(),
            playback: receiver,
        }
    }
}

/// A unique simulation created from a [`SimulationSet`].
// TODO:
// - Should this store information about what was computed?
#[derive(Component)]
pub struct Simulation {
    pub name: String,
    pub set: Entity,
    pub world: World,
    pub playback: Receiver<PlaybackUpdate>,
}

// TODO:
// - Should Simulation be a child of SimulationSet?
impl Simulation {
    fn run(sender: Sender<PlaybackUpdate>, start_time: SimulationTime, end_time: SimulationTime) {
        // Temporary implementation
        let mut time = start_time;
        while time < end_time {
            std::thread::sleep(Self::get_random_sleep_duration());
            if sender.send(PlaybackUpdate::SomeUpdate(time)).is_err() {
                return;
            }
            time = SimulationTime::new(time.elapsed() + Duration::from_secs(1));
        }
    }

    // TODO: Temporary implementation to remove
    fn get_random_sleep_duration() -> Duration {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.subsec_nanos())
            .unwrap_or(0);
        Duration::from_millis((nanos % 20) as u64)
    }
}

#[derive(Debug, Clone)]
pub enum PlaybackUpdate {
    SomeUpdate(SimulationTime),
}
