use crate::sync::EntityCloner;
use crate::time::SimulationTime;
use bevy::ecs::system::ScheduleSystem;
use bevy::{prelude::*, tasks::AsyncComputeTaskPool};
use crossbeam_channel::{Receiver, Sender, unbounded};

/// Plugin for running discrete event simulations.
pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
    }
}

/// A set of entities, components, systems, and resources from which a Simulation can be run.
#[derive(Component)]
pub struct SimulationBuilder {
    startup_schedule: Schedule,
    compute_schedule: Schedule,
}

impl SimulationBuilder {
    // WARN: If these default labels are mistakenly on the main world, they will override the existing schedules.
    pub fn new() -> Self {
        Self {
            startup_schedule: Schedule::new(Startup),
            compute_schedule: Schedule::new(Update),
        }
    }

    pub fn build(
        self,
        name: String,
        set: Entity,
        end_condition: EndCondition,
        entity_cloner: &mut EntityCloner,
        main_world: &World,
    ) -> Simulation {
        let (update_sender, update_receiver) = unbounded::<StateUpdate>();
        let mut sim_world = World::new();
        entity_cloner.clone_to_sim(main_world, &mut sim_world);
        let mut sim_run = SimulationRun::new(
            sim_world,
            end_condition.clone(),
            self.startup_schedule,
            self.compute_schedule,
            update_sender,
        );
        AsyncComputeTaskPool::get()
            .spawn(async move { sim_run.run() })
            .detach();
        Simulation {
            name,
            set,
            end_condition,
            update_receiver,
        }
    }

    pub fn add_startup_systems<M>(
        mut self,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> Self {
        self.startup_schedule.add_systems(systems);
        self
    }

    pub fn add_compute_systems<M>(
        mut self,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> Self {
        self.compute_schedule.add_systems(systems);
        self
    }
}

/// A condition that determines when the simulation should end.
// TODO: Make this a query, and then EndCondition::time can be a simple check on the clock resource.
#[derive(Debug, Clone)]
pub enum EndCondition {
    Time(SimulationTime),
}

/// A reference object for a [`SimulationRun`] executed in a separate thread.
#[derive(Component)]
pub struct Simulation {
    pub name: String,
    pub set: Entity,
    pub end_condition: EndCondition,
    pub update_receiver: Receiver<StateUpdate>,
}

/// A running simulation in a separate thread.
pub struct SimulationRun {
    world: World,
    pub end_condition: EndCondition,
    startup_schedule: Schedule,
    compute_schedule: Schedule,
    update_sender: Sender<StateUpdate>,
}

impl SimulationRun {
    pub fn new(
        world: World,
        end_condition: EndCondition,
        startup_schedule: Schedule,
        compute_schedule: Schedule,
        update_sender: Sender<StateUpdate>,
    ) -> Self {
        SimulationRun {
            world,
            end_condition,
            startup_schedule,
            compute_schedule,
            update_sender,
        }
    }

    pub fn run(&mut self) {
        self.startup_schedule.run(&mut self.world);
        self.compute_schedule.run(&mut self.world);
    }
}

// TODO: A placeholder implementation
#[derive(Debug, Clone)]
pub struct StateUpdate(SimulationTime);
