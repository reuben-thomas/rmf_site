use bevy::ecs::schedule::{
    InternedScheduleLabel, IntoScheduleConfigs, Schedule, ScheduleConfigs, ScheduleLabel,
};
use bevy::ecs::system::ScheduleSystem;

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

#[derive(Default)]
pub enum SystemExecutionOrdering {
    #[default]
    Partial,
    Total,
}

type ConfigFactory = Box<dyn FnOnce() -> ScheduleConfigs<ScheduleSystem> + Send + Sync>;

/// Builds a [`Schedule`] with a set of systems and an execution ordering policy.
pub struct ScheduleBuilder {
    label: InternedScheduleLabel,
    factories: Vec<ConfigFactory>,
    ordering: SystemExecutionOrdering,
}

impl ScheduleBuilder {
    pub fn new(label: impl ScheduleLabel) -> Self {
        Self {
            label: label.intern(),
            factories: Vec::new(),
            ordering: SystemExecutionOrdering::default(),
        }
    }

    pub fn add_systems<M>(&mut self, systems: impl SimulationScheduleConfigs<M>) -> &mut Self {
        self.factories
            .push(Box::new(move || systems.into_configs()));
        self
    }

    pub fn set_ordering(&mut self, ordering: SystemExecutionOrdering) -> &mut Self {
        self.ordering = ordering;
        self
    }

    pub fn build(self) -> Schedule {
        let Self { label, factories, ordering } = self;
        let mut schedule = Schedule::new(label);
        Self::initialize_systems(factories, &ordering, &mut schedule);
        Self::initialize_ordering(&ordering, &mut schedule);
        schedule
    }

    fn initialize_systems(
        factories: Vec<ConfigFactory>,
        ordering: &SystemExecutionOrdering,
        schedule: &mut Schedule,
    ) {
        let configs = factories.into_iter().map(|f| f());
        match ordering {
            SystemExecutionOrdering::Total => {
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

    fn initialize_ordering(_ordering: &SystemExecutionOrdering, _schedule: &mut Schedule) {}
}
