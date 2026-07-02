use crate::compute::{SimulationComputePlugin, compute_simulation};
use crate::event::DiscreteEventsPlugin;
use crate::schedule::{
    ScheduleBuilder, SimulationCompute, SimulationComputeStep, SimulationScheduleConfigs,
    SimulationSetup, SystemExecutionOrdering,
};
use crate::sync::{EntitySynchronizer, StepSender};
use crate::time::SimulationTime;
use bevy::app::SubApp;
use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::sync::Arc;

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
    setup_schedule_builder: ScheduleBuilder,
    compute_schedule_builder: ScheduleBuilder,
    compute_plugin_configs: Vec<SubAppPluginConfig>,
}

impl SimulationBuilder {
    pub fn new() -> Self {
        Self {
            synchronizer: EntitySynchronizer::new(),
            setup_schedule_builder: ScheduleBuilder::new(SimulationSetup),
            compute_schedule_builder: ScheduleBuilder::new(SimulationComputeStep)
                .set_ordering(SystemExecutionOrdering::Total)
                .in_set(SimulationCompute::ExecuteSystems),
            compute_plugin_configs: Vec::new(),
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

    pub fn register_event<T: Event>(mut self) -> Self {
        self.compute_plugin_configs.push(Arc::new(|sub_app| {
            sub_app.add_plugins(DiscreteEventsPlugin::<T>::default());
        }));
        self
    }

    pub fn add_setup_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.setup_schedule_builder = self.setup_schedule_builder.add_systems(systems);
        self
    }

    pub fn add_compute_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.compute_schedule_builder = self.compute_schedule_builder.add_systems(systems);
        self
    }

    pub fn build(&self, world: &World) -> Simulation {
        let entities = self.synchronizer.extract_entities(world);
        let init_step = SimulationInitStep {
            commands: self.synchronizer.extract_components(world),
        };
        let (step_sender, step_receiver) = unbounded();
        let sub_app = self.build_compute_sub_app(step_sender);

        let compute_task = AsyncComputeTaskPool::get().spawn(compute_simulation(
            sub_app,
            entities,
            init_step.clone(),
        ));

        Simulation {
            init_step,
            steps: Vec::new(),
            step_receiver,
            compute_task,
        }
    }

    fn build_compute_sub_app(&self, step_sender: Sender<SimulationStep>) -> SubApp {
        let mut sub_app = SubApp::new();
        sub_app.insert_resource(StepSender::new(step_sender));
        sub_app.add_plugins((
            SimulationComputePlugin,
            self.synchronizer.clone(),
            self.setup_schedule_builder.clone(),
            self.compute_schedule_builder.clone(),
        ));
        for install in &self.compute_plugin_configs {
            install(&mut sub_app);
        }
        sub_app.finish();
        sub_app.cleanup();
        sub_app
    }
}

// TODO: Remember not to make this a type unless duplicated across sites.
type SubAppPluginConfig = Arc<dyn Fn(&mut SubApp) + Send + Sync>;

#[derive(Component)]
pub struct Simulation {
    init_step: SimulationInitStep,
    steps: Vec<SimulationStep>,
    step_receiver: Receiver<SimulationStep>,
    // TODO: Better way to keep task alive?
    compute_task: Task<()>,
}

impl Simulation {
    // TODO: Time bound this system in order to avoid delaying the main app. Alternatively, bound channel.
    fn update_steps(mut simulations: Query<&mut Simulation>) {
        for mut simulation in &mut simulations {
            let simulation = &mut *simulation;
            for step in simulation.step_receiver.try_iter() {
                simulation.steps.push(step);
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
// - This is a terrible data structure to efficiently store changes.
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
