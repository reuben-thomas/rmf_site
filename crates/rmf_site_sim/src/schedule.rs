use bevy::ecs::schedule::{
    InternedScheduleLabel, IntoScheduleConfigs, LogLevel, NodeId, Schedule, ScheduleBuildError,
    ScheduleBuildPass, ScheduleBuildSettings, ScheduleConfigs, ScheduleGraph, ScheduleLabel,
    SystemSet, graph::DiGraph,
};
use bevy::ecs::system::ScheduleSystem;
use bevy::ecs::world::World;

/// The schedule that runs once when the a simulation is started.
#[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct SimulationStartup;

#[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct SimulationModelSystemExec;

#[derive(Default, Clone)]
pub enum SystemExecutionOrdering {
    /// Systems may run in any order based on dependencies and user-defined ordering constraints.
    #[default]
    Partial,
    /// A deterministic total order is enforced.
    Total,
}

/// Builds a [`Schedule`] with a set of systems and an execution ordering policy.
pub struct ScheduleBuilder {
    label: InternedScheduleLabel,
    configs: Option<ScheduleConfigs<ScheduleSystem>>,
    ordering: SystemExecutionOrdering,
}

impl ScheduleBuilder {
    /// Creates a new empty builder for a schedule.
    pub fn new(label: impl ScheduleLabel) -> Self {
        Self {
            label: label.intern(),
            configs: None,
            ordering: SystemExecutionOrdering::default(),
        }
    }

    /// Adds systems to the schedule.
    pub fn add_systems<M>(mut self, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) -> Self {
        let configs = systems.into_configs();
        self.configs = Some(match self.configs.take() {
            Some(existing) => (existing, configs).into_configs(),
            None => configs,
        });
        self
    }

    /// Adds systems to the schedule for a system set.
    pub fn add_systems_in_set<M>(
        self,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
        set: impl SystemSet,
    ) -> Self {
        self.add_systems(systems.in_set(set))
    }

    /// Sets the execution ordering policy for the schedule.
    pub fn set_ordering(mut self, ordering: SystemExecutionOrdering) -> Self {
        self.ordering = ordering;
        self
    }

    /// Builds a new [`Schedule`] containing the added systems, ordered by the configured policy.
    pub fn build(self) -> Schedule {
        let mut schedule = Schedule::new(self.label);
        schedule.set_build_settings(ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            hierarchy_detection: LogLevel::Warn,
            auto_insert_apply_deferred: true,
            use_shortnames: true,
            report_sets: true,
        });
        if let Some(configs) = self.configs {
            schedule.add_systems(configs);
        }
        match self.ordering {
            SystemExecutionOrdering::Total => {
                schedule.add_build_pass(TotalOrderingPass);
            }
            SystemExecutionOrdering::Partial => {}
        }
        schedule
    }
}

/// A schedule build pass that enforces a deterministic total ordering of systems.
#[derive(Debug)]
struct TotalOrderingPass;

impl ScheduleBuildPass for TotalOrderingPass {
    type EdgeOptions = ();

    fn add_dependency(&mut self, _from: NodeId, _to: NodeId, _options: Option<&Self::EdgeOptions>) {
    }

    fn collapse_set(
        &mut self,
        _set: NodeId,
        _systems: &[NodeId],
        _dependency_flattened: &DiGraph,
    ) -> impl Iterator<Item = (NodeId, NodeId)> {
        std::iter::empty()
    }

    fn build(
        &mut self,
        _world: &mut World,
        _graph: &mut ScheduleGraph,
        _dependency_flattened: &mut DiGraph,
    ) -> Result<(), ScheduleBuildError> {
        // TODO: Enforce a total ordering of the systems in the schedule
        Ok(())
    }
}
