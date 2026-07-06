use crate::compute::{SimulationComputePlugin, compute_simulation};
use crate::event::DiscreteEventsPlugin;
use crate::schedule::{
    ScheduleBuilder, SimulationComputeSet, SimulationComputeStep, SimulationScheduleConfigs,
    SimulationStartup, SystemExecutionOrdering,
};
use crate::sync::EntitySynchronizer;
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
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_systems(Update, Simulation::update_steps);
    }
}

/// Adds a set of plugins to the target world's [`App`].
pub type PluginFactory = Arc<dyn Fn(&mut App) + Send + Sync>;

/// Builds a [`Simulation`].
#[derive(Clone, Component)]
pub struct SimulationBuilder {
    synchronizer: EntitySynchronizer,
    startup_schedule_builder: ScheduleBuilder,
    compute_schedule_builder: ScheduleBuilder,
    plugins: Vec<PluginFactory>,
}

impl Default for SimulationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulationBuilder {
    pub fn new() -> Self {
        Self {
            synchronizer: EntitySynchronizer::default(),
            startup_schedule_builder: ScheduleBuilder::new(SimulationStartup),
            compute_schedule_builder: ScheduleBuilder::new(SimulationComputeStep)
                .set_ordering(SystemExecutionOrdering::Total),
            plugins: Vec::new(),
        }
    }

    pub fn add_plugins<M>(
        mut self,
        plugins: impl Plugins<M> + Clone + Send + Sync + 'static,
    ) -> Self {
        self.plugins.push(Arc::new(move |app| {
            app.add_plugins(plugins.clone());
        }));
        self
    }

    pub fn register_tracked_component<T: Component + Clone>(mut self) -> Self {
        self.synchronizer.register_tracked::<T>();
        self
    }

    pub fn register_untracked_component<T: Component + Clone>(mut self) -> Self {
        self.synchronizer.register_untracked::<T>();
        self
    }

    pub fn add_startup_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.startup_schedule_builder = self.startup_schedule_builder.add_systems(systems);
        self
    }

    pub fn add_compute_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.compute_schedule_builder = self
            .compute_schedule_builder
            .add_systems_in_set(systems, SimulationComputeSet::ExecuteSystems);
        self
    }

    /// Registers a discrete event type that can be scheduled in the simulation, driving the
    /// compute clock forward to each scheduled event time.
    pub fn register_event<T: Event>(self) -> Self {
        self.add_plugins(DiscreteEventsPlugin::<T>::default())
    }

    pub fn build(&self, world: &World) -> Simulation {
        Simulation::new(self.clone(), world)
    }
}

#[derive(Component)]
pub struct Simulation {
    init_step: SimulationInitStep,
    simulation_steps: Vec<SimulationStep>,
    step_receiver: Receiver<SimulationStep>,
}

impl Simulation {
    fn new(builder: SimulationBuilder, world: &World) -> Self {
        let entities = builder.synchronizer.extract_entities(world);
        let init_step = SimulationInitStep {
            commands: builder.synchronizer.extract_components(world),
        };
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

    // TODO: Time bound this system in order to avoid delaying the main app.
    fn update_steps(mut simulations: Query<&mut Simulation>) {
        for mut simulation in &mut simulations {
            let simulation = &mut *simulation;
            for step in simulation.step_receiver.try_iter() {
                simulation.simulation_steps.push(step);
            }
        }
    }
}

#[derive(Clone)]
pub struct SimulationStep {
    pub time: SimulationTime,
    pub commands: Vec<Box<dyn SimulationCommand>>,
}

#[derive(Clone)]
pub struct SimulationInitStep {
    pub commands: Vec<Box<dyn SimulationCommand>>,
}

// TODO:
// - Just enforce Command + Sync instead?
// - This is a terrible data structure to store changes
pub trait SimulationCommand: Send + Sync + 'static {
    fn apply(self: Box<Self>, world: &mut World);
    fn clone_box(&self) -> Box<dyn SimulationCommand>;
}

impl Clone for Box<dyn SimulationCommand> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

pub struct ComponentChanges<T: Component>(pub EntityHashMap<T>);

impl<T: Component + Clone> SimulationCommand for ComponentChanges<T> {
    fn apply(self: Box<Self>, world: &mut World) {
        for (entity, value) in self.0 {
            if let Ok(mut e) = world.get_entity_mut(entity) {
                e.insert(value);
            }
        }
    }

    fn clone_box(&self) -> Box<dyn SimulationCommand> {
        Box::new(ComponentChanges(self.0.clone()))
    }
}
