use bevy::ecs::schedule::{IntoScheduleConfigs, Schedule, ScheduleLabel};
use bevy::ecs::system::ScheduleSystem;
use std::sync::Arc;

/// The schedule that runs once when the a simulation is started.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SimulationStartup;

/// The schedule containing systems representing models in the simulation, that must be run once per simulation step.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SimulationComputeStep;

pub trait SimulationScheduleConfigs<M>:
    IntoScheduleConfigs<ScheduleSystem, M> + Clone + Send + Sync + 'static
{
}

impl<M, T> SimulationScheduleConfigs<M> for T where
    T: IntoScheduleConfigs<ScheduleSystem, M> + Clone + Send + Sync + 'static
{
}

/// Adds a set of systems to a [`Schedule`] when creating a simulation.
#[derive(Clone)]
pub struct ScheduleInitializer {
    init: Arc<dyn Fn(&mut Schedule) + Send + Sync>,
}

impl ScheduleInitializer {
    pub fn new<M>(systems: impl SimulationScheduleConfigs<M>) -> Self {
        Self {
            init: Arc::new(move |schedule: &mut Schedule| {
                schedule.add_systems(systems.clone());
            }),
        }
    }

    pub fn initialize(&self, schedule: &mut Schedule) {
        let init = &self.init;
        init(schedule);
    }
}
