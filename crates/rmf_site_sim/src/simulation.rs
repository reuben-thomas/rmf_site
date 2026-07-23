use crate::compute::compute_async;
use crate::event::DiscreteEvent;
use crate::schedule::{
    ScheduleBuilder, SimulationStartup, SimulationSystemExec, SystemExecutionOrdering,
};
use crate::sync::{SimulationState, Synchronizer};
use crate::time::SimulationTime;
use bevy::app::Plugins;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, unbounded};
use std::collections::BTreeMap;
use std::marker::PhantomData;

/// Plugin for computing discrete event simulations.
pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, Simulation::process_updates);
    }
}

/// Adds one or more plugins to the target world's [`App`].
pub type SimulationPluginFactory = Box<dyn FnOnce(&mut App) + Send>;

/// Builds a [`Simulation`].
pub struct SimulationBuilder<M: Component + Clone> {
    synchronizer: Synchronizer,
    marker: PhantomData<M>,
    startup_schedule_builder: ScheduleBuilder,
    system_schedule_builder: ScheduleBuilder,
    plugins: Vec<SimulationPluginFactory>,
}

impl<M: Component + Clone> Default for SimulationBuilder<M> {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating a [`Simulation`].
///
/// All registered resources and components, as well as their corresponding entities with the
/// marker component `M` are extracted into the simulation world.
impl<M: Component + Clone> SimulationBuilder<M> {
    pub fn new() -> Self {
        let mut synchronizer = Synchronizer::new();
        synchronizer.register_component::<M>();
        Self {
            synchronizer,
            startup_schedule_builder: ScheduleBuilder::new(SimulationStartup),
            system_schedule_builder: ScheduleBuilder::new(SimulationSystemExec)
                .set_ordering(SystemExecutionOrdering::Total),
            plugins: Vec::new(),
            marker: PhantomData,
        }
    }

    /// Registers a component to synchronize into the simulation world.
    pub fn register_component<T: Component + Clone>(mut self) -> Self {
        self.synchronizer.register_component::<T>();
        self
    }

    /// Registers a resource to synchronize into the simulation world.
    pub fn register_resource<T: Resource + Clone>(mut self) -> Self {
        self.synchronizer.register_resource::<T>();
        self
    }

    /// Adds a plugin to the simulation [`App`].
    pub fn add_plugins<P>(mut self, plugins: impl Plugins<P> + Send + 'static) -> Self {
        self.plugins.push(Box::new(move |app| {
            app.add_plugins(plugins);
        }));
        self
    }

    /// Adds a set of systems to be executed during simulation startup.
    pub fn add_startup_systems<S>(
        mut self,
        systems: impl IntoScheduleConfigs<ScheduleSystem, S>,
    ) -> Self {
        self.startup_schedule_builder = self.startup_schedule_builder.add_systems(systems);
        self
    }

    /// Adds a set of systems to be executed when computing simulation steps.
    /// These systems should represent models in the discrete event simulation.
    pub fn add_simulation_systems<S>(
        mut self,
        systems: impl IntoScheduleConfigs<ScheduleSystem, S>,
    ) -> Self {
        self.system_schedule_builder = self.system_schedule_builder.add_systems(systems);
        self
    }

    /// Builds a new [`Simulation`].
    pub fn build(self, world: &World) -> Simulation {
        Simulation::new(self, world)
    }
}

/// A unique discrete event simulation.
#[derive(Component)]
pub struct Simulation {
    init_state: SimulationState,
    synchronizer: Synchronizer,
    simulation_steps: BTreeMap<SimulationTime, SimulationStep>,
    state: SimulationComputeState,
    update_receiver: Receiver<SimulationComputeUpdate>,
}

impl Simulation {
    // TODO: Should extract be performed explicitly? e.g. Simulation::extract, Simulation::compute
    fn new<M: Component + Clone>(builder: SimulationBuilder<M>, world: &World) -> Self {
        let (sender, receiver) = unbounded();
        compute_async(
            SimulationState::extract_with::<M>(&builder.synchronizer, world),
            builder.startup_schedule_builder.build(),
            builder.system_schedule_builder.build(),
            builder.plugins,
            builder.synchronizer.clone(),
            sender,
        );

        Self {
            init_state: SimulationState::extract_with::<M>(&builder.synchronizer, world),
            synchronizer: builder.synchronizer,
            simulation_steps: BTreeMap::new(),
            state: SimulationComputeState::Computing,
            update_receiver: receiver,
        }
    }

    pub fn state(&self) -> SimulationComputeState {
        self.state
    }

    pub fn init_state(&self) -> &SimulationState {
        &self.init_state
    }

    pub fn synchronizer(&self) -> &Synchronizer {
        &self.synchronizer
    }

    // TODO: Should this provide an iterator instead?
    /// The computed simulation steps, ordered by simulation time.
    pub fn steps(&self) -> &BTreeMap<SimulationTime, SimulationStep> {
        &self.simulation_steps
    }

    // TODO: Time bound this system in order to avoid delaying the main app.
    /// Applies [`StateUpdate`]s received from the compute task.
    fn process_updates(mut simulations: Query<&mut Simulation>) {
        for mut simulation in &mut simulations {
            let simulation = &mut *simulation;
            for update in simulation.update_receiver.try_iter() {
                match update {
                    SimulationComputeUpdate::Step(time, step) => {
                        simulation.simulation_steps.insert(time, step);
                    }
                    SimulationComputeUpdate::State(state) => {
                        simulation.state = state;
                    }
                }
            }
        }
    }
}

/// The current state of a simulation's computation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimulationComputeState {
    Computing,
    Complete,
    Failed,
}

/// A single computed simulation step.
#[derive(Clone)]
pub struct SimulationStep {
    pub events: Vec<Box<dyn DiscreteEvent>>,
}

impl SimulationStep {
    pub fn apply(self, world: &mut World) {
        for event in self.events {
            event.apply(world);
        }
    }
}

pub enum SimulationComputeUpdate {
    Step(SimulationTime, SimulationStep),
    State(SimulationComputeState),
}
