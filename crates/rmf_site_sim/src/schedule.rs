use bevy::ecs::schedule::{
    InternedScheduleLabel, IntoScheduleConfigs, Schedule, ScheduleConfigs, ScheduleLabel,
};
use bevy::ecs::system::ScheduleSystem;
use std::sync::Arc;

/// The schedule that runs once when the a simulation is started.
#[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct SimulationStartup;

/// The schedule containing systems representing models in the simulation, that must be run once per simulation step.
#[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct SimulationComputeStep;

/// The schedule that runs after each [`SimulationComputeStep`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct SimulationPostComputeStep;

pub trait SimulationScheduleConfigs<M>:
    IntoScheduleConfigs<ScheduleSystem, M> + Clone + Send + Sync + 'static
{
}

impl<M, T> SimulationScheduleConfigs<M> for T where
    T: IntoScheduleConfigs<ScheduleSystem, M> + Clone + Send + Sync + 'static
{
}

#[derive(Default, Clone)]
pub enum SystemExecutionOrdering {
    #[default]
    Partial,
    Total,
}

// TODO: Arc is not necessary if deciding that SimulationBuilder need not be cloneable, or a component
type ScheduleConfigFactory = Arc<dyn Fn() -> ScheduleConfigs<ScheduleSystem> + Send + Sync>;

/// Builds a [`Schedule`] with a set of systems and an execution ordering policy.
#[derive(Clone)]
pub struct ScheduleBuilder {
    label: InternedScheduleLabel,
    schedule_config_factories: Vec<ScheduleConfigFactory>,
    ordering: SystemExecutionOrdering,
}

impl ScheduleBuilder {
    pub fn new(label: impl ScheduleLabel) -> Self {
        Self {
            label: label.intern(),
            schedule_config_factories: Vec::new(),
            ordering: SystemExecutionOrdering::default(),
        }
    }

    pub fn add_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.schedule_config_factories
            .push(Arc::new(move || systems.clone().into_configs()));
        self
    }

    pub fn set_ordering(mut self, ordering: SystemExecutionOrdering) -> Self {
        self.ordering = ordering;
        self
    }

    pub fn build(&self) -> Schedule {
        let mut schedule = Schedule::new(self.label);
        Self::initialize_systems(
            &self.schedule_config_factories,
            &self.ordering,
            &mut schedule,
        );
        Self::initialize_ordering(&self.ordering, &mut schedule);
        schedule
    }

    fn initialize_systems(
        factories: &Vec<ScheduleConfigFactory>,
        ordering: &SystemExecutionOrdering,
        schedule: &mut Schedule,
    ) {
        let configs = factories.iter().map(|f| f());
        match ordering {
            SystemExecutionOrdering::Total => {
                // TODO:
                // - Consider a default ordering by system names
                // - A chain should not conflict existing ordering bounds
                if let Some(chained) = configs.reduce(|a, b| (a, b).chain()) {
                    schedule.add_systems(chained);
                }
            }
            SystemExecutionOrdering::Partial => {
                for config in configs {
                    schedule.add_systems(config);
                }
            }
        }
    }

    fn initialize_ordering(_ordering: &SystemExecutionOrdering, _schedule: &mut Schedule) {
        // TODO
    }
}
