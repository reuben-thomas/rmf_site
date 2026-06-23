use bevy::ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy::ecs::system::ScheduleSystem;
use std::sync::Arc;

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
