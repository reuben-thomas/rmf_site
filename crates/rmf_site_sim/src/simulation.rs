use crate::compute::{SimulationComputePlugin, compute_simulation};
use crate::event::DiscreteEventsPlugin;
use crate::schedule::{
    ScheduleBuilder, SimulationComputeSet, SimulationComputeStep, SimulationScheduleConfigs,
    SimulationStartup, SystemExecutionOrdering,
};
use crate::sync::Synchronizer;
use crate::time::SimulationTime;
use bevy::app::Plugins;
use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, unbounded};
use std::sync::Arc;
use std::thread;

/// Plugin for computing discrete event simulations.
pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, Simulation::update_steps);
    }
}

/// Builds a [`Simulation`].
#[derive(Clone, Component)]
pub struct SimulationBuilder {
    synchronizer: Synchronizer,
    startup_schedule_builder: ScheduleBuilder,
    compute_schedule_builder: ScheduleBuilder,
    plugins: Vec<PluginFactory>,
}

impl Default for SimulationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating a [`Simulation`].
///
/// All registererd resources and components, as well as their corresponding entities are extracted into the simulation world.
/// Only tracked components and resources are tracked for changes and sent as a [`SimulationStep`] to the main world.
impl SimulationBuilder {
    pub fn new() -> Self {
        Self {
            synchronizer: Synchronizer::default(),
            startup_schedule_builder: ScheduleBuilder::new(SimulationStartup),
            compute_schedule_builder: ScheduleBuilder::new(SimulationComputeStep)
                .set_ordering(SystemExecutionOrdering::Total),
            plugins: Vec::new(),
        }
    }

    /// Adds a plugin to the simulation [`SubApp`].
    pub fn add_plugins<M>(
        mut self,
        plugins: impl Plugins<M> + Clone + Send + Sync + 'static,
    ) -> Self {
        self.plugins.push(Arc::new(move |app| {
            app.add_plugins(plugins.clone());
        }));
        self
    }

    /// Registers a tracked component.
    pub fn register_tracked_component<T: Component + Clone>(mut self) -> Self {
        self.synchronizer.register_tracked::<T>();
        self
    }

    /// Registers an untracked component.
    pub fn register_untracked_component<T: Component + Clone>(mut self) -> Self {
        self.synchronizer.register_untracked::<T>();
        self
    }

    /// Registers a tracked resource.
    pub fn register_tracked_resource<T: Resource + Clone>(mut self) -> Self {
        self.synchronizer.register_tracked_resource::<T>();
        self
    }

    /// Registers an untracked resource.
    pub fn register_untracked_resource<T: Resource + Clone>(mut self) -> Self {
        self.synchronizer.register_untracked_resource::<T>();
        self
    }

    /// Adds a set of systems to be executed during simulation startup.
    pub fn add_startup_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.startup_schedule_builder = self.startup_schedule_builder.add_systems(systems);
        self
    }

    // TODO: Rename to add_model_systems instead?
    /// Adds a set of systems to be executed when computing simulation steps.
    /// These systems should represent models in the discrete event simulation.
    pub fn add_compute_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.compute_schedule_builder = self
            .compute_schedule_builder
            .add_systems_in_set(systems, SimulationComputeSet::ExecuteSystems);
        self
    }

    /// Registers a discrete event type that can be scheduled in the simulation, driving the
    /// compute clock forward to each scheduled event time.
    pub fn register_discrete_event<T: Event>(self) -> Self {
        self.add_plugins(DiscreteEventsPlugin::<T>::default())
    }

    /// Builds a new [`Simulation`].
    pub fn build(&self, world: &World) -> Simulation {
        Simulation::new(self.clone(), world)
    }
}

/// A unique discrete event simulation.
#[derive(Component)]
pub struct Simulation {
    init_step: SimulationInitStep,
    simulation_steps: Vec<SimulationStep>,
    step_receiver: Receiver<SimulationStep>,
}

impl Simulation {
    // TODO: Should extract be performed explicitly? e.g. Simulation::extract, Simulation::compute
    fn new(builder: SimulationBuilder, world: &World) -> Self {
        let entities = builder.synchronizer.extract_entities(world);
        let mut commands = builder.synchronizer.extract_components(world);
        commands.extend(builder.synchronizer.extract_resources(world));
        let init_step = SimulationInitStep { commands };
        let (step_sender, step_receiver) = unbounded();
        let compute_plugin = SimulationComputePlugin {
            synchronizer: builder.synchronizer,
            startup_schedule_builder: builder.startup_schedule_builder,
            compute_schedule_builder: builder.compute_schedule_builder,
            entities,
            init_step: init_step.clone(),
            step_sender,
        };

        thread::spawn(move || {
            compute_simulation(compute_plugin, builder.plugins);
        });

        Self {
            step_receiver,
            init_step,
            simulation_steps: Vec::new(),
        }
    }

    /// The initial step of the simulation, which can be used to reset to the simulation's initial state.
    pub fn init_step(&self) -> &SimulationInitStep {
        &self.init_step
    }

    /// The computed simulation steps, in order of execution.
    pub fn steps(&self) -> &[SimulationStep] {
        &self.simulation_steps
    }

    // TODO: Time bound this system in order to avoid delaying the main app.
    /// Updates the simulation steps from the step receiver.
    fn update_steps(mut simulations: Query<&mut Simulation>) {
        for mut simulation in &mut simulations {
            let simulation = &mut *simulation;
            for step in simulation.step_receiver.try_iter() {
                simulation.simulation_steps.push(step);
            }
        }
    }
}

// TODO:
// - Perhaps handle world mutations directly in a step
// - Possibly better as a generic, e.g. SimulationStep<Init>, SimulationStep<Update>
/// A single computed simulation step.
#[derive(Clone)]
pub struct SimulationStep {
    pub time: SimulationTime,
    pub commands: Vec<Box<dyn SimulationCommand>>,
}

/// A simulation step that that captures the initial state of the simulation
#[derive(Clone)]
pub struct SimulationInitStep {
    pub commands: Vec<Box<dyn SimulationCommand>>,
}

// TODO: This data structure is inefficient for storing changes, since unique changes are all stored in closures.
/// A world mutation similar to [`Command`].
/// Unlike [`Command`], this trait requires `Send` + `Sync` instead of just `Send` to allow a simulation to be computed in a separate thread.
pub trait SimulationCommand: Send + Sync + 'static {
    /// Applies the command to the world.
    fn apply(self: Box<Self>, world: &mut World);

    /// Clones into a boxed object.
    fn clone_to_box(&self) -> Box<dyn SimulationCommand>;
}

impl Clone for Box<dyn SimulationCommand> {
    fn clone(&self) -> Self {
        self.clone_to_box()
    }
}

/// A map of [`Entity`] to changed [`Component`] values.
pub struct ComponentChanges<T: Component>(pub EntityHashMap<T>);

impl<T: Component + Clone> SimulationCommand for ComponentChanges<T> {
    fn apply(self: Box<Self>, world: &mut World) {
        for (entity, value) in self.0 {
            if let Ok(mut e) = world.get_entity_mut(entity) {
                e.insert(value);
            }
        }
    }

    fn clone_to_box(&self) -> Box<dyn SimulationCommand> {
        Box::new(ComponentChanges(self.0.clone()))
    }
}

/// A changed [`Resource`] value.
pub struct ResourceChanges<T: Resource>(pub T);

impl<T: Resource + Clone> SimulationCommand for ResourceChanges<T> {
    fn apply(self: Box<Self>, world: &mut World) {
        world.insert_resource(self.0);
    }

    fn clone_to_box(&self) -> Box<dyn SimulationCommand> {
        Box::new(ResourceChanges(self.0.clone()))
    }
}

/// Adds one or more plugins to the target world's [`App`].
pub type PluginFactory = Arc<dyn Fn(&mut App) + Send + Sync>;
