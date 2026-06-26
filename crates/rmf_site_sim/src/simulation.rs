use crate::schedule::{
    ScheduleBuilder, SimulationComputeStep, SimulationScheduleConfigs, SimulationStartup,
};
use crate::sync::EntityCloner;
use bevy::{prelude::*, tasks::AsyncComputeTaskPool};
use std::sync::{Arc, Mutex};

/// Plugin for computing discrete event simulations.
pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
    }
}

/// Builds a [`Simulation`].
pub struct SimulationBuilder {
    entity_cloner: EntityCloner,
    startup_builder: ScheduleBuilder,
    compute_builder: ScheduleBuilder,
}

impl SimulationBuilder {
    pub fn new() -> Self {
        Self {
            entity_cloner: EntityCloner::default(),
            startup_builder: ScheduleBuilder::new(SimulationStartup),
            compute_builder: ScheduleBuilder::new(SimulationComputeStep),
        }
    }

    pub fn register_component<T: Component + Clone>(mut self) -> Self {
        self.entity_cloner.register::<T>();
        self
    }

    pub fn add_startup_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.startup_builder.add_systems(systems);
        self
    }

    pub fn add_compute_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.compute_builder.add_systems(systems);
        self
    }

    pub fn register_event<T: Event>(self) -> Self {
        todo!()
    }

    pub fn build(self) -> Simulation {
        let startup_schedule = self.startup_builder.build();
        let compute_schedule = self.compute_builder.build();

        let world = Arc::new(Mutex::new(World::new()));
        let run = Arc::new(Mutex::new(SimulationRun::new(
            Arc::clone(&world),
            startup_schedule,
            compute_schedule,
        )));

        Simulation {
            entity_cloner: self.entity_cloner,
            world,
            run,
        }
    }
}

/// A configured simulation, ready to sync and run.
#[derive(Component)]
pub struct Simulation {
    entity_cloner: EntityCloner,
    world: Arc<Mutex<World>>,
    run: Arc<Mutex<SimulationRun>>,
}

impl Simulation {
    pub fn sync_from_world(&mut self, world: &World) {
        let mut sim_world = self.world.lock().unwrap();
        self.entity_cloner.clone_to_sim(world, &mut sim_world);
    }

    pub fn sync_to_world(&mut self, _world: &mut World) {
        let _sim_world = self
            .world
            .lock()
            .expect("Failed to lock world for sync_to_world");
    }

    pub fn compute_async(&mut self) {
        let sim_run = Arc::clone(&self.run);
        AsyncComputeTaskPool::get()
            .spawn(async move { sim_run.lock().unwrap().run() })
            .detach();
    }
}

/// A simulation run that is executed in a separate thread.
struct SimulationRun {
    world: Arc<Mutex<World>>,
    startup_schedule: Schedule,
    compute_schedule: Schedule,
}

impl SimulationRun {
    fn new(
        world: Arc<Mutex<World>>,
        startup_schedule: Schedule,
        compute_schedule: Schedule,
    ) -> Self {
        SimulationRun {
            world,
            startup_schedule,
            compute_schedule,
        }
    }

    pub fn run(&mut self) {
        {
            let mut world = self.world.lock().unwrap();
            self.startup_schedule.run(&mut world);
        }
        {
            let mut world = self.world.lock().unwrap();
            self.compute_schedule.run(&mut world);
        }
    }
}
