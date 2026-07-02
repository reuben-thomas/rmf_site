use crate::compute::compute_simulation;
use crate::schedule::{
    ScheduleBuilder, SimulationComputeSet, SimulationComputeStep, SimulationScheduleConfigs,
    SimulationStartup, SystemExecutionOrdering,
};
use crate::sync::EntitySynchronizer;
use crate::time::SimulationTime;
use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, unbounded};
use std::thread;

/// Plugin for computing discrete event simulations.
pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_systems(Update, Simulation::update_steps);
    }
}

/// Builds a [`Simulation`].
#[derive(Clone, Component)]
pub struct SimulationBuilder {
    synchronizer: EntitySynchronizer,
    startup_schedule_builder: ScheduleBuilder,
    compute_schedule_builder: ScheduleBuilder,
}

impl SimulationBuilder {
    pub fn new() -> Self {
        Self {
            synchronizer: EntitySynchronizer::default(),
            startup_schedule_builder: ScheduleBuilder::new(SimulationStartup),
            compute_schedule_builder: ScheduleBuilder::new(SimulationComputeStep)
                .set_ordering(SystemExecutionOrdering::Total),
        }
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

    pub fn register_event<T: Event>(self) -> Self {
        todo!()
    }

    pub fn build(&self, world: &World) -> Simulation {
        Simulation::new(
            self.synchronizer.clone(),
            self.startup_schedule_builder.build(),
            self.compute_schedule_builder.build(),
            world,
        )
    }
}

#[derive(Component)]
pub struct Simulation {
    init_step: SimulationInitStep,
    simulation_steps: Vec<SimulationStep>,
    step_receiver: Receiver<SimulationStep>,
}

impl Simulation {
    fn new(
        synchronizer: EntitySynchronizer,
        startup_schedule: Schedule,
        system_schedule: Schedule,
        world: &World,
    ) -> Self {
        let entities = synchronizer.extract_entities(world);
        let init_step = SimulationInitStep {
            commands: synchronizer.extract_components(world),
        };
        let compute_init_step = init_step.clone();
        let (step_sender, step_receiver) = unbounded();

        thread::spawn(move || {
            compute_simulation(
                startup_schedule,
                system_schedule,
                synchronizer,
                entities,
                compute_init_step,
                step_sender,
            );
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
