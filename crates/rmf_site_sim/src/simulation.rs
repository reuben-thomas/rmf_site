use crate::time::SimulationTime;
use bevy::{prelude::*, tasks::AsyncComputeTaskPool};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::sync::{Arc, Mutex};

/// Plugin for running discrete event simulations.
pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
    }
}

/// A set of entities, components, systems, and resources from which a Simulation can be run.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SimulationSet;

impl SimulationSet {
    pub fn run(
        &self,
        name: String,
        set: Entity,
        end_condition: EndCondition,
        schedule: Schedule,
    ) -> Simulation {
        Simulation::new(name, set, end_condition, schedule)
    }
}

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
    run: Arc<Mutex<SimulationRun>>,
    pub update_receiver: Receiver<StateUpdate>,
}

impl Simulation {
    pub fn new(name: String, set: Entity, end_condition: EndCondition, schedule: Schedule) -> Self {
        let (update_sender, update_receiver) = unbounded::<StateUpdate>();
        let run = Arc::new(Mutex::new(SimulationRun::new(
            end_condition.clone(),
            schedule,
            update_sender,
        )));
        let run_task = run.clone();
        AsyncComputeTaskPool::get()
            .spawn(async move {
                info!("Running simulation");
                run_task.lock().unwrap().run();
            })
            .detach();
        Simulation {
            name,
            set,
            end_condition,
            run,
            update_receiver,
        }
    }
}

/// A running simulation in a separate thread.
pub struct SimulationRun {
    world: World,
    pub end_condition: EndCondition,
    schedule: Schedule,
    update_sender: Sender<StateUpdate>,
}

impl SimulationRun {
    pub fn new(
        end_condition: EndCondition,
        schedule: Schedule,
        update_sender: Sender<StateUpdate>,
    ) -> Self {
        SimulationRun {
            world: World::new(),
            end_condition: end_condition,
            schedule,
            update_sender,
        }
    }

    pub fn run(&mut self) {
        self.schedule.run(&mut self.world);
    }
}

// TODO: A placeholder implementation
#[derive(Debug, Clone)]
pub struct StateUpdate(SimulationTime);
