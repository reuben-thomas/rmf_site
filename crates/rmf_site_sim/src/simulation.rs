use crate::compute::compute_simulation;
use crate::extract::EntityExtractor;
use crate::schedule::{
    ScheduleBuilder, SimulationComputeStep, SimulationScheduleConfigs, SimulationStartup,
    SystemExecutionOrdering,
};
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
    extractor: EntityExtractor,
    startup_schedule_builder: ScheduleBuilder,
    compute_schedule_builder: ScheduleBuilder,
}

impl SimulationBuilder {
    pub fn new() -> Self {
        Self {
            extractor: EntityExtractor::default(),
            startup_schedule_builder: ScheduleBuilder::new(SimulationStartup),
            compute_schedule_builder: ScheduleBuilder::new(SimulationComputeStep)
                .set_ordering(SystemExecutionOrdering::Total),
        }
    }

    pub fn register_component<T: Component + Clone>(mut self) -> Self {
        self.extractor.register::<T>();
        self
    }

    pub fn add_startup_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.startup_schedule_builder = self.startup_schedule_builder.add_systems(systems);
        self
    }

    pub fn add_compute_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.compute_schedule_builder = self.compute_schedule_builder.add_systems(systems);
        self
    }

    pub fn register_event<T: Event>(self) -> Self {
        todo!()
    }

    pub fn build(&self, world: &World) -> Simulation {
        Simulation::new(
            self.extractor.clone(),
            self.startup_schedule_builder.build(),
            self.compute_schedule_builder.build(),
            world,
        )
    }
}

#[derive(Component)]
pub struct Simulation {
    initial_step: Option<SimulationStep>,
    computed_steps: Vec<SimulationStep>,
    step_receiver: Receiver<SimulationStep>,
}

impl Simulation {
    fn new(
        extractor: EntityExtractor,
        startup_schedule: Schedule,
        compute_schedule: Schedule,
        world: &World,
    ) -> Self {
        let entities = extractor.extract_entities(world);
        let initial_step = SimulationStep {
            time: SimulationTime::default(),
            commands: extractor.extract_components(world),
        };
        let seed_step = SimulationStep {
            time: SimulationTime::default(),
            commands: extractor.extract_components(world),
        };

        let (step_sender, step_receiver) = unbounded();

        thread::spawn(move || {
            compute_simulation(
                startup_schedule,
                compute_schedule,
                extractor,
                entities,
                seed_step,
                step_sender,
            );
        });

        Self {
            step_receiver,
            initial_step: Some(initial_step),
            computed_steps: Vec::new(),
        }
    }

    // TODO: Time bound this system in order to avoid delaying the main app.
    fn update_steps(mut simulations: Query<&mut Simulation>) {
        for mut simulation in &mut simulations {
            let simulation = &mut *simulation;
            for step in simulation.step_receiver.try_iter() {
                simulation.computed_steps.push(step);
            }
        }
    }
}

pub struct SimulationStep {
    pub time: SimulationTime,
    pub commands: Vec<Box<dyn SimulationCommand>>,
}

// TODO:
// - Just enforce Command + Sync instead? https://docs.rs/bevy/latest/bevy/ecs/prelude/trait.Command.html
// - This is a terrible data structure to actually store changes
pub trait SimulationCommand: Send + Sync + 'static {
    fn apply(self: Box<Self>, world: &mut World);
}

pub struct ComponentChanges<T: Component>(pub EntityHashMap<T>);

impl<T: Component> SimulationCommand for ComponentChanges<T> {
    fn apply(self: Box<Self>, world: &mut World) {
        for (entity, value) in self.0 {
            if let Ok(mut e) = world.get_entity_mut(entity) {
                e.insert(value);
            }
        }
    }
}
