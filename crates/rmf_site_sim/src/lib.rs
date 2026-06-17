use bevy::ecs::component::ComponentId;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

pub use compute::*;
pub use event::*;

mod compute;
mod event;

pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ComputePlugin);
    }
}

/// Type that can be used as a simulation time.
pub trait SimTime: Ord + Hash + Copy + Send + Sync + Debug + 'static {}

// Blanket implementation for any type that satisfies the required trait bounds to be used as a [`SimTime`].
impl<T: Ord + Hash + Copy + Send + Sync + Debug + 'static> SimTime for T {}

/// A set of entities, components, systems, and resources that can be used to compute a simulation.
#[derive(Component)]
pub struct SimulationSet<T: SimTime> {
    setup_schedule: Schedule,
    compute_schedule: Schedule,
    untracked_components: HashSet<ComponentId>,
    tracked_components: HashSet<ComponentId>,
    _time: PhantomData<T>,
}

impl<T: SimTime> SimulationSet<T> {
    fn new() -> Self {
        Self {
            setup_schedule: Schedule::new(SetupSimulation),
            compute_schedule: Schedule::new(ComputeTimeStep),
            untracked_components: HashSet::default(),
            tracked_components: HashSet::default(),
            _time: PhantomData,
        }
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SetupSimulation;

pub struct SimulationSetBuilder<'a, T: SimTime> {
    world: &'a mut World,
    simulation_set: SimulationSet<T>,
}

impl<'a, T: SimTime> SimulationSetBuilder<'a, T> {
    pub fn new(world: &'a mut World) -> Self {
        Self {
            world,
            simulation_set: SimulationSet::new(),
        }
    }

    pub fn add_setup_systems<M>(
        mut self,
        system: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> Self {
        self.simulation_set.setup_schedule.add_systems(system);
        self
    }

    // TODO:
    // - Add a deterministic total ordering for systems unless otherwise specified in the schedule configs
    pub fn add_systems<M>(mut self, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) -> Self {
        self.simulation_set.compute_schedule.add_systems(systems);
        self
    }

    pub fn register_tracked_component<C: Component>(mut self) -> Self {
        let id = self.world.register_component::<C>();
        self.simulation_set.untracked_components.remove(&id);
        self.simulation_set.tracked_components.insert(id);
        self
    }

    // TODO:
    // - Can events be automatically registered from the provided systems?
    pub fn register_event<E: DiscreteEvent<Time = T>>(mut self) -> Self
    where
        T: Default,
    {
        if !self.world.contains_resource::<ComputeClock<E::Time>>() {
            self.world.init_resource::<ComputeClock<E::Time>>();
            self.simulation_set
                .compute_schedule
                .add_systems(advance_clock::<E::Time>.run_if(in_state(ComputeState::Computing)));
        }

        self.world.init_resource::<DiscreteEvents<E>>();
        self.simulation_set
            .compute_schedule
            .add_systems(update_clock::<E>.before(advance_clock::<E::Time>));
        self
    }

    fn register_system_components_as_untracked(&mut self) {
        if self
            .simulation_set
            .compute_schedule
            .initialize(self.world)
            .is_err()
        {
            panic!("Unable to initialize schedule with world");
        }
        let Ok(systems) = self.simulation_set.compute_schedule.systems() else {
            panic!("Unable to retrieve systems from schedule");
        };

        let accessed: Vec<ComponentId> = systems
            .filter_map(|(_, system)| system.component_access().try_iter_component_access().ok())
            .flatten()
            .map(|access| *access.index())
            .collect();

        for id in accessed {
            if !self.simulation_set.tracked_components.contains(&id) {
                self.simulation_set.untracked_components.insert(id);
            }
        }
    }

    pub fn build(mut self) -> SimulationSet<T> {
        self.register_system_components_as_untracked();
        self.simulation_set
    }
}
