use bevy::ecs::component::ComponentId;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use std::collections::HashSet;

pub use compute::*;
pub use event::*;
pub use time::*;

mod compute;
mod event;
mod simulation;
mod time;

pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ComputePlugin);
    }
}

/// A unique simulation instance computed on a [`SimulationSet`].
#[derive(Component)]
pub struct Simulation {
    world: World,
}

/// A set of entities, components, systems, and resources in a [`World`] that can be used to compute a simulation.
#[derive(Component)]
pub struct SimulationSet {
    startup_schedule: Schedule,
    compute_schedule: Schedule,
    untracked_components: HashSet<ComponentId>,
    tracked_components: HashSet<ComponentId>,
}

impl SimulationSet {
    fn new() -> Self {
        let mut compute_schedule = Schedule::new(ComputeTimeStep);
        compute_schedule
            .configure_sets(SimulationSystems::Clock.before(SimulationSystems::Compute));
        Self {
            // WARN: If these default labels are mistakenly on the main world, they will override the existing schedules.
            startup_schedule: Schedule::new(Startup),
            compute_schedule: Schedule::new(Update),
            untracked_components: HashSet::default(),
            tracked_components: HashSet::default(),
        }
    }
}

/// A builder for creating a [`SimulationSet`].
pub struct SimulationSetBuilder<'a> {
    world: &'a mut World,
    simulation_set: SimulationSet,
}

impl<'a> SimulationSetBuilder<'a> {
    pub fn new(world: &'a mut World) -> Self {
        Self {
            world,
            simulation_set: SimulationSet::new(),
        }
    }

    pub fn add_startup_systems<M>(
        mut self,
        system: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> Self {
        self.simulation_set.startup_schedule.add_systems(system);
        self
    }

    // TODO:
    // - Add a deterministic total ordering for systems unless otherwise specified in the schedule configs
    pub fn add_compute_systems<M>(
        mut self,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> Self {
        self.simulation_set
            .compute_schedule
            .add_systems(systems.in_set(SimulationSystems::Compute));
        self
    }

    // TODO:
    // - Can events, components be automatically registered from the provided systems?
    pub fn register_untracked_component<C: Component>(mut self) -> Self {
        let component_id = self.world.register_component::<C>();
        self.simulation_set
            .untracked_components
            .insert(component_id);
        self
    }

    pub fn register_tracked_component<C: Component>(mut self) -> Self {
        let component_id = self.world.register_component::<C>();
        self.simulation_set
            .untracked_components
            .remove(&component_id);
        self.simulation_set.tracked_components.insert(component_id);
        self
    }

    pub fn register_event<E: Event>(mut self) -> Self {
        self
    }

    pub fn build(self) -> SimulationSet {
        self.simulation_set
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SimulationSystems {
    Clock,
    Compute,
}
