use crate::compute::{SimulationComputePlugin, compute_simulation};
use crate::event::DiscreteEvent;
use crate::schedule::{
    ScheduleBuilder, SimulationComputeSet, SimulationComputeStep, SimulationScheduleConfigs,
    SimulationStartup, SystemExecutionOrdering,
};
use crate::sync::Extractor;
use crate::time::SimulationTime;
use bevy::app::Plugins;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, unbounded};
use std::collections::BTreeMap;
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
    extractor: Extractor,
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
/// All registered resources and components, as well as their corresponding entities are extracted into the simulation world.
impl SimulationBuilder {
    pub fn new() -> Self {
        Self {
            extractor: Extractor::default(),
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

    /// Builds a new [`Simulation`].
    pub fn build(&self, world: &World) -> Simulation {
        Simulation::new(self.clone(), world)
    }
}

/// A unique discrete event simulation.
#[derive(Component)]
pub struct Simulation {
    init_step: SimulationStep,
    simulation_steps: BTreeMap<SimulationTime, SimulationStep>,
    step_receiver: Receiver<(SimulationTime, SimulationStep)>,
}

impl Simulation {
    // TODO: Should extract be performed explicitly? e.g. Simulation::extract, Simulation::compute
    fn new(builder: SimulationBuilder, world: &World) -> Self {
        let entities = builder.extractor.extract_entities(world);
        let events = builder.extractor.create_extract_event(world);
        let init_step = SimulationStep { events };
        let (step_sender, step_receiver) = unbounded();
        let compute_plugin = SimulationComputePlugin {
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
            simulation_steps: BTreeMap::new(),
        }
    }

    /// The initial step of the simulation, which can be used to reset to the simulation's initial state.
    pub fn init_step(&self) -> &SimulationStep {
        &self.init_step
    }

    /// The computed simulation steps, ordered by simulation time.
    pub fn steps(&self) -> &BTreeMap<SimulationTime, SimulationStep> {
        &self.simulation_steps
    }

    // TODO: Time bound this system in order to avoid delaying the main app.
    /// Updates the simulation steps from the step receiver.
    fn update_steps(mut simulations: Query<&mut Simulation>) {
        for mut simulation in &mut simulations {
            let simulation = &mut *simulation;
            for (time, step) in simulation.step_receiver.try_iter() {
                simulation.simulation_steps.insert(time, step);
            }
        }
    }
}

/// A single computed simulation step.
#[derive(Clone)]
pub struct SimulationStep {
    pub events: Vec<Box<dyn DiscreteEvent>>,
}

/// Adds one or more plugins to the target world's [`App`].
pub type PluginFactory = Arc<dyn Fn(&mut App) + Send + Sync>;
