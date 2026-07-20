use crate::compute::compute_async;
use crate::event::DiscreteEvent;
use crate::schedule::{
    ScheduleBuilder, SimulationModelSystemExec, SimulationStartup, SystemExecutionOrdering,
};
use crate::sync::Extractor;
use crate::time::SimulationTime;
use bevy::app::Plugins;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use bevy::utils::synccell::SyncCell;
use crossbeam_channel::{Receiver, unbounded};
use std::collections::BTreeMap;

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
pub struct SimulationBuilder<M: Component> {
    extractor: Extractor<M>,
    startup_schedule_builder: ScheduleBuilder,
    model_system_schedule_builder: ScheduleBuilder,
    plugins: Vec<SimulationPluginFactory>,
}

impl<M: Component> Default for SimulationBuilder<M> {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating a [`Simulation`].
///
/// All registered resources and components, as well as their corresponding entities with the
/// marker component `M` are extracted into the simulation world.
impl<M: Component> SimulationBuilder<M> {
    pub fn new() -> Self {
        Self {
            extractor: Extractor::default(),
            startup_schedule_builder: ScheduleBuilder::new(SimulationStartup),
            model_system_schedule_builder: ScheduleBuilder::new(SimulationModelSystemExec)
                .set_ordering(SystemExecutionOrdering::Total),
            plugins: Vec::new(),
        }
    }

    /// Registers a component to extract into the simulation world.
    pub fn register_component<T: Component + Clone>(mut self) -> Self {
        self.extractor.register_component::<T>();
        self
    }

    /// Registers a resource to extract into the simulation world.
    pub fn register_resource<T: Resource + Clone>(mut self) -> Self {
        self.extractor.register_resource::<T>();
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
    pub fn add_simulation_model_systems<S>(
        mut self,
        systems: impl IntoScheduleConfigs<ScheduleSystem, S>,
    ) -> Self {
        self.model_system_schedule_builder =
            self.model_system_schedule_builder.add_systems(systems);
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
    init_step: SyncCell<SimulationStep>,
    simulation_steps: SyncCell<BTreeMap<SimulationTime, SimulationStep>>,
    state: SimulationState,
    update_receiver: Receiver<SimulationComputeUpdate>,
}

impl Simulation {
    // TODO: Should extract be performed explicitly? e.g. Simulation::extract, Simulation::compute
    fn new<M: Component>(builder: SimulationBuilder<M>, world: &World) -> Self {
        let entities = builder.extractor.extract_entities(world);
        let init_step = SimulationStep {
            events: builder.extractor.create_extract_events(world),
        };
        let (update_sender, update_receiver) = unbounded();
        compute_async(
            update_sender,
            builder.startup_schedule_builder.build(),
            builder.model_system_schedule_builder.build(),
            entities,
            init_step.clone(),
            builder.plugins,
        );

        Self {
            init_step: SyncCell::new(init_step),
            simulation_steps: SyncCell::new(BTreeMap::new()),
            state: SimulationState::Computing,
            update_receiver,
        }
    }

    pub fn state(&self) -> SimulationState {
        self.state
    }

    /// The initial step of the simulation, which can be used to reset to the simulation's initial state.
    pub fn init_step(&mut self) -> &SimulationStep {
        self.init_step.get()
    }

    /// The computed simulation steps, ordered by simulation time.
    pub fn steps(&mut self) -> &BTreeMap<SimulationTime, SimulationStep> {
        self.simulation_steps.get()
    }

    // TODO: Time bound this system in order to avoid delaying the main app.
    /// Applies [`StateUpdate`]s received from the compute task.
    fn process_updates(mut simulations: Query<&mut Simulation>) {
        for mut simulation in &mut simulations {
            let simulation = &mut *simulation;
            for update in simulation.update_receiver.try_iter() {
                match update {
                    SimulationComputeUpdate::Step(time, step) => {
                        simulation.simulation_steps.get().insert(time, step);
                    }
                    SimulationComputeUpdate::State(state) => {
                        simulation.state = state;
                    }
                }
            }
        }
    }
}

/// The current state of a simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimulationState {
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
    State(SimulationState),
}
