use crate::schedule::SimulationComputeSet;
use crate::simulation::{SimulationInitStep, SimulationStep};
use crate::sync::EntitySynchronizer;
use crate::time::SimulationTime;
use bevy::prelude::*;
use crossbeam_channel::Sender;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

// TODO: Error handling, this is being executed in a separate thread.
pub fn compute_simulation(
    mut startup_schedule: Schedule,
    mut system_schedule: Schedule,
    synchronizer: EntitySynchronizer,
    entities: Vec<Entity>,
    init_step: SimulationInitStep,
    step_sender: Sender<SimulationStep>,
) {
    let mut world = create_world(entities, init_step);
    world.init_resource::<SimulationClock>();

    system_schedule.configure_sets(
        SimulationComputeSet::ExecuteSystems.before(SimulationComputeSet::SendSimulationStep),
    );
    system_schedule
        .add_systems(SimulationClock::advance.in_set(SimulationComputeSet::SendSimulationStep));
    synchronizer.configure_tracking(&mut world, &mut system_schedule, step_sender);

    startup_schedule.run(&mut world);
    run_systems(system_schedule, &mut world);
}

fn create_world(entities: Vec<Entity>, init_step: SimulationInitStep) -> World {
    let mut world = World::new();
    for entity in entities {
        spawn_at(&mut world, entity);
    }
    for command in init_step.commands {
        command.apply(&mut world);
    }
    world
}

fn run_systems(mut system_schedule: Schedule, world: &mut World) {
    loop {
        system_schedule.run(world);

        // TODO: This should be a generic trait with a builtin impl,
        // e.g. crate::simulation::EndCondition
        if world.resource::<SimulationClock>().at_end() {
            break;
        }
    }
}

#[derive(Resource, Default)]
pub struct SimulationClock {
    current: SimulationTime,
    pending: BinaryHeap<Reverse<SimulationTime>>,
}

impl SimulationClock {
    pub fn now(&self) -> SimulationTime {
        self.current
    }

    pub fn add(&mut self, time: SimulationTime) {
        // TODO: Better error handling than panicking
        if time <= self.current {
            panic!(
                "Tried to add time {time:?} that is not greater than the current time {:?}.",
                self.now()
            )
        }
        self.pending.push(Reverse(time));
    }

    fn at_end(&self) -> bool {
        self.pending.is_empty()
    }

    fn next(&mut self) -> Option<SimulationTime> {
        if self.at_end() {
            return None;
        }

        let Reverse(time) = self.pending.pop()?;
        self.current = time;
        Some(time)
    }

    pub fn advance(mut clock: ResMut<SimulationClock>) {
        if clock.at_end() {
            info!("Compute clock reached end at time {:?}", clock.now());
            return;
        }

        clock.next();
    }
}

// TODO: This method is only in newer versions of Bevy.
fn spawn_at(world: &mut World, entity: Entity) {
    #[allow(deprecated)]
    let _ = world.insert_or_spawn_batch([(entity, ())]);
}
