use bevy::app::{App, Plugin};
use bevy::ecs::schedule::{
    ExecutorKind, InternedScheduleLabel, InternedSystemSet, IntoScheduleConfigs, ScheduleConfigs,
    ScheduleLabel, SystemSet,
};
use bevy::ecs::system::ScheduleSystem;
use std::sync::Arc;

/// The schedule that runs once when the a simulation is started.
#[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct SimulationSetup;

/// The schedule that runs once per simulation step.
#[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct SimulationComputeStep;

/// The system sets that structure a single [`SimulationComputeStep`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, SystemSet)]
pub enum SimulationCompute {
    ExecuteSystems,
    BufferChangedComponents,
    SendSimulationStep,
    ScheduleNextStep,
    IncrementComputeClock,
}

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

type ScheduleConfigFactory = Arc<dyn Fn() -> ScheduleConfigs<ScheduleSystem> + Send + Sync>;

/// A [`Plugin`] that adds a set of systems to the schedule identified by `label`,
/// with an execution ordering policyjkjjjGkk.
#[derive(Clone)]
pub struct ScheduleBuilder {
    label: InternedScheduleLabel,
    schedule_config_factories: Vec<ScheduleConfigFactory>,
    ordering: SystemExecutionOrdering,
    system_set: Option<InternedSystemSet>,
}

impl ScheduleBuilder {
    pub fn new(label: impl ScheduleLabel) -> Self {
        Self {
            label: label.intern(),
            schedule_config_factories: Vec::new(),
            ordering: SystemExecutionOrdering::default(),
            system_set: None,
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

    /// Places all systems added to this builder in the given [`SystemSet`].
    pub fn in_set(mut self, set: impl SystemSet) -> Self {
        self.system_set = Some(set.intern());
        self
    }
}

impl Plugin for ScheduleBuilder {
    fn build(&self, app: &mut App) {
        let configs = self
            .schedule_config_factories
            .iter()
            .map(|factory| match self.system_set {
                Some(set) => factory().in_set(set),
                None => factory(),
            });

        match &self.ordering {
            SystemExecutionOrdering::Total => {
                // TODO:
                // - Consider a default ordering by system names
                // - A chain should not conflict existing ordering bounds
                let chained_systems = configs.reduce(|a, b| (a, b).chain()).unwrap();
                app.add_systems(self.label, chained_systems);
                // TODO:
                // - A single threaded executor does not guarantee total ordering across runs
                // - Implement a custom https://docs.rs/bevy/0.16.1/bevy/ecs/schedule/trait.ScheduleBuildPass.html
                app.edit_schedule(self.label, |schedule| {
                    schedule.set_executor_kind(ExecutorKind::SingleThreaded);
                });
            }
            SystemExecutionOrdering::Partial => {
                for config in configs {
                    app.add_systems(self.label, config);
                }
            }
        }
    }

    fn is_unique(&self) -> bool {
        false
    }
}
