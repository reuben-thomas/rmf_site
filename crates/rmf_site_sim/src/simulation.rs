use crate::schedule::{
    ScheduleInitializer, SimulationComputeStep, SimulationScheduleConfigs, SimulationStartup,
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
#[derive(Component, Default)]
pub struct SimulationBuilder {
    entity_cloner: EntityCloner,
    startup_system_initializers: Vec<ScheduleInitializer>,
    compute_system_initializers: Vec<ScheduleInitializer>,
}

impl SimulationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_component<T: Component + Clone>(mut self) -> Self {
        self.entity_cloner.register::<T>();
        self
    }

    pub fn add_startup_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.startup_system_initializers
            .push(ScheduleInitializer::new(systems));
        self
    }

    pub fn add_compute_systems<M>(mut self, systems: impl SimulationScheduleConfigs<M>) -> Self {
        self.compute_system_initializers
            .push(ScheduleInitializer::new(systems));
        self
    }

    pub fn register_event<T: Event>(self) -> Self {
        todo!()
    }

    pub fn build(self) -> Simulation {
        Simulation {
            entity_cloner: self.entity_cloner,
            startup_system_initializers: self.startup_system_initializers,
            compute_system_initializers: self.compute_system_initializers,
            sim_world: Arc::new(Mutex::new(World::new())),
            run: None,
        }
    }
}

/// A configured simulation, ready to sync and run.
#[derive(Component)]
pub struct Simulation {
    entity_cloner: EntityCloner,
    startup_system_initializers: Vec<ScheduleInitializer>,
    compute_system_initializers: Vec<ScheduleInitializer>,
    sim_world: Arc<Mutex<World>>,
    run: Option<Arc<Mutex<SimulationRun>>>,
}

impl Simulation {
    pub fn sync_from_world(&mut self, world: &World) {
        let mut sim_world = self.sim_world.lock().unwrap();
        self.entity_cloner.clone_to_sim(world, &mut sim_world);
    }

    pub fn sync_to_world(&mut self, world: &mut World) {
        todo!();
    }

    pub fn run_async(&mut self) {
        let mut startup_schedule = Schedule::new(SimulationStartup);
        for init in &self.startup_system_initializers {
            init.initialize(&mut startup_schedule);
        }
        let mut compute_schedule = Schedule::new(SimulationComputeStep);
        for init in &self.compute_system_initializers {
            init.initialize(&mut compute_schedule);
        }

        let sim_run = Arc::new(Mutex::new(SimulationRun::new(
            Arc::clone(&self.sim_world),
            startup_schedule,
            compute_schedule,
        )));
        self.run = Some(Arc::clone(&sim_run));

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
        let mut world = self.world.lock().unwrap();
        self.startup_schedule.run(&mut world);
        self.compute_schedule.run(&mut world);
    }
}
