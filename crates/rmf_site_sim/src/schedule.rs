use bevy::ecs::schedule::{
    InternedScheduleLabel, IntoScheduleConfigs, NodeId, Schedule, ScheduleBuildError,
    ScheduleBuildPass, ScheduleConfigs, ScheduleGraph, ScheduleLabel, SystemSet,
    graph::{DiGraph, Direction},
};
use bevy::ecs::system::ScheduleSystem;
use bevy::ecs::world::World;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// The schedule that runs once when the a simulation is started.
#[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct SimulationStartup;

/// The schedule containing systems representing models in the simulation, that must be run once per simulation step.
#[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct SimulationComputeStep;

/// System sets that order execution within a [`SimulationComputeStep`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
pub enum SimulationComputeSet {
    ExecuteSystems,
    SendSimulationStep,
    IncrementComputeClock,
}

/// Systems that can be added to a simulation schedule.
pub trait SimulationScheduleConfigs<M>:
    IntoScheduleConfigs<ScheduleSystem, M> + Clone + Send + Sync + 'static
{
}

impl<M, T> SimulationScheduleConfigs<M> for T where
    T: IntoScheduleConfigs<ScheduleSystem, M> + Clone + Send + Sync + 'static
{
}

/// Determines how system executions are ordered within a [`SimulationComputeStep`].
#[derive(Default, Clone)]
pub enum SystemExecutionOrdering {
    /// Systems may run in any order based on dependencies and user-defined ordering constraints.
    #[default]
    Partial,
    /// A deterministic total order is enforced.
    Total,
}

type ScheduleConfigFactory = Arc<dyn Fn() -> ScheduleConfigs<ScheduleSystem> + Send + Sync>;

/// Builds a [`Schedule`] with a set of systems and an execution ordering policy.
#[derive(Clone)]
pub struct ScheduleBuilder {
    label: InternedScheduleLabel,
    schedule_config_factories: Vec<ScheduleConfigFactory>,
    ordering: SystemExecutionOrdering,
}

impl ScheduleBuilder {
    /// Creates a new empty builder for a schedule.
    pub fn new(label: impl ScheduleLabel) -> Self {
        Self {
            label: label.intern(),
            schedule_config_factories: Vec::new(),
            ordering: SystemExecutionOrdering::default(),
        }
    }

    /// Adds systems to the schedule.
    pub fn add_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.schedule_config_factories
            .push(Arc::new(move || systems.clone().into_configs()));
        self
    }

    /// Adds systems to the schedule for a system set.
    pub fn add_systems_in_set<M>(
        mut self,
        systems: impl SimulationScheduleConfigs<M>,
        set: impl SystemSet + Clone,
    ) -> Self {
        self.schedule_config_factories.push(Arc::new(move || {
            systems.clone().into_configs().in_set(set.clone())
        }));
        self
    }

    /// Sets the execution ordering policy for the schedule.
    pub fn set_ordering(mut self, ordering: SystemExecutionOrdering) -> Self {
        self.ordering = ordering;
        self
    }

    /// Builds a new [`Schedule`] containing the added systems, ordered by the configured policy.
    pub fn build(&self) -> Schedule {
        let mut schedule = Schedule::new(self.label);
        for factory in &self.schedule_config_factories {
            schedule.add_systems(factory());
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
        graph: &mut ScheduleGraph,
        dependency_flattened: &mut DiGraph,
    ) -> Result<(), ScheduleBuildError> {
        println!("Hierarchy graph: {:?}", graph.hierarchy().graph());
        println!(
            "Hierarchy topsort: {:?}",
            graph.hierarchy().cached_topsort()
        );
        println!("Dependency graph: {:?}", graph.dependency().graph());
        println!(
            "Dependency topsort: {:?}",
            graph.dependency().cached_topsort()
        );

        let systems: Vec<(NodeId, &ScheduleSystem)> = graph
            .systems()
            .map(|(id, system, _)| (id, system))
            .collect();

        let reachable: HashMap<NodeId, HashSet<NodeId>> = systems
            .iter()
            .map(|&(id, _)| (id, reachable_from(dependency_flattened, id)))
            .collect();

        for (i, &(a, system_a)) in systems.iter().enumerate() {
            for &(b, system_b) in &systems[i + 1..] {
                if reachable[&a].contains(&b) || reachable[&b].contains(&a) {
                    continue;
                }
                let conflicting = system_a.is_exclusive()
                    || system_b.is_exclusive()
                    || !system_a
                        .component_access()
                        .is_compatible(system_b.component_access());
                if conflicting {
                    println!(
                        "Conflicting systems: {} and {}",
                        system_a.name(),
                        system_b.name()
                    );
                }
            }
        }

        Ok(())
    }
}

fn reachable_from(graph: &DiGraph, start: NodeId) -> HashSet<NodeId> {
    let mut visited = HashSet::new();
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        for next in graph.neighbors_directed(node, Direction::Outgoing) {
            if visited.insert(next) {
                stack.push(next);
            }
        }
    }
    visited
}
