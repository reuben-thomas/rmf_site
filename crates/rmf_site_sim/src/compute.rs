use crate::schedule::{SimulationCompute, SimulationComputeStep, SimulationSetup};
use crate::simulation::SimulationInitStep;
use crate::time::SimulationTime;
use bevy::app::SubApp;
use bevy::prelude::*;
use bevy::tasks::futures_lite::future;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub struct SimulationComputePlugin;
const YIELD_AFTER_STEPS: u32 = 64;

impl Plugin for SimulationComputePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimulationComputeClock>()
            .init_schedule(SimulationSetup)
            .init_schedule(SimulationComputeStep)
            .configure_sets(
                SimulationComputeStep,
                (
                    SimulationCompute::ExecuteSystems,
                    SimulationCompute::BufferChangedComponents,
                    SimulationCompute::SendSimulationStep,
                    SimulationCompute::ScheduleNextStep,
                    SimulationCompute::IncrementComputeClock,
                )
                    .chain(),
            )
            .add_systems(
                SimulationComputeStep,
                SimulationComputeClock::advance.in_set(SimulationCompute::IncrementComputeClock),
            );
    }
}

// TODO: Error handling, this is being executed in a task pool.
pub async fn compute_simulation(
    mut sub_app: SubApp,
    entities: Vec<Entity>,
    init_step: SimulationInitStep,
) {
    let world = sub_app.world_mut();
    for entity in entities {
        spawn_at(world, entity);
    }
    for command in init_step.commands {
        command.apply(world);
    }
    world.run_schedule(SimulationSetup);

    let mut steps = 0u32;
    loop {
        world.run_schedule(SimulationComputeStep);
        world.clear_trackers();

        // TODO: This should be a generic trait with a builtin impl,
        // e.g. crate::simulation::EndCondition
        if world.resource::<SimulationComputeClock>().at_end() {
            break;
        }

        steps += 1;
        if steps.is_multiple_of(YIELD_AFTER_STEPS) {
            future::yield_now().await;
        }
    }
}

#[derive(Resource, Default)]
pub struct SimulationComputeClock {
    current: SimulationTime,
    pending: BinaryHeap<Reverse<SimulationTime>>,
}

impl SimulationComputeClock {
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

    pub fn advance(mut clock: ResMut<SimulationComputeClock>) {
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
