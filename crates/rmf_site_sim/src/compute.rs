use crate::event::{DiscreteEvents, execute_first_instant_event, execute_scheduled_events};
use crate::schedule::{SimulationModelSystemExec, SimulationStartup};
use crate::simulation::{PluginFactory, SimulationComputeUpdate, SimulationState, SimulationStep};
use crate::sync::{SimulationEventBuffer, StateUpdateSender};
use crate::time::SimulationTime;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, TaskPool};
use crossbeam_channel::Sender;
use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Computes [`SimulationStep`]s for a simulation in a [`AsyncComputeTaskPool`].
pub fn compute_async(
    compute_plugin: SimulationComputePlugin,
    init_entities: Vec<Entity>,
    init_step: SimulationStep,
    plugins: Vec<PluginFactory>,
) {
    let update_sender = compute_plugin.update_sender.clone();
    AsyncComputeTaskPool::get_or_init(TaskPool::new)
        .spawn(async move {
            let result = catch_unwind(AssertUnwindSafe(move || {
                let mut app = App::new();
                compute_plugin.build(&mut app);

                let world = app.world_mut();
                for entity in &init_entities {
                    spawn_at(world, *entity);
                }
                init_step.apply(world);
                for plugin in plugins {
                    plugin(&mut app);
                }
                app.run();
            }));
            let state = match result {
                Ok(()) => SimulationState::Complete,
                Err(_) => SimulationState::Failed,
            };
            let _ = update_sender.send(SimulationComputeUpdate::State(state));
        })
        .detach();
}

/// Configures an [`App`] to compute [`SimulationStep`]s for a simulation.
pub struct SimulationComputePlugin {
    pub startup_schedule: Schedule,
    pub system_schedule: Schedule,
    pub update_sender: Sender<SimulationComputeUpdate>,
}

impl SimulationComputePlugin {
    fn build(self, app: &mut App) {
        app.init_resource::<SimulationComputeClock>()
            .init_resource::<DiscreteEvents>()
            .init_resource::<SimulationEventBuffer>()
            .insert_resource(StateUpdateSender::new(self.update_sender));

        app.add_schedule(self.startup_schedule);
        app.add_schedule(self.system_schedule);
        app.set_runner(run_compute_simulation);
    }
}

fn run_compute_simulation(mut app: App) -> AppExit {
    let world = app.world_mut();
    world.run_schedule(SimulationStartup);
    loop {
        compute_time_step(world);
        let _ = world.run_system_cached(SimulationEventBuffer::send_step);
        let _ = world.run_system_cached(sync_with_clock);
        let _ = world.run_system_cached(SimulationComputeClock::advance);

        // TODO: This should be a generic trait with a builtin impl,
        // e.g. crate::simulation::EndCondition
        if world.resource::<SimulationComputeClock>().is_complete() {
            break;
        }
    }
    AppExit::Success
}

fn compute_time_step(world: &mut World) {
    execute_scheduled_events(world);
    loop {
        world.run_schedule(SimulationModelSystemExec);
        if !execute_first_instant_event(world) {
            break;
        }
    }
}

pub fn sync_with_clock(events: Res<DiscreteEvents>, mut clock: ResMut<SimulationComputeClock>) {
    if let Some(time) = events.next_time() {
        clock.try_add_pending(time);
    }
}

/// Clock for tracking the current simulation time being computed, as well as pending times to be processed.
#[derive(Resource, Default)]
pub struct SimulationComputeClock {
    current: SimulationTime,
    pending: BTreeSet<SimulationTime>,
    is_complete: bool,
}

impl SimulationComputeClock {
    /// The current simulation time.
    pub fn now(&self) -> SimulationTime {
        self.current
    }

    /// Adds a pending time to be processed.
    ///
    /// Returns whether a new pending time was added.
    pub fn try_add_pending(&mut self, time: SimulationTime) -> bool {
        if time <= self.current {
            panic!(
                "Tried to add time {time:?} that is not greater than the current time {:?}.",
                self.now()
            )
        }
        self.pending.insert(time)
    }

    /// Whether all pending times have been processed.
    fn is_complete(&self) -> bool {
        self.is_complete
    }

    /// Ticks the clock to the next pending time.
    fn tick(&mut self) {
        let time = self
            .pending
            .pop_first()
            .expect("No pending times to increment");
        self.current = time;
    }

    /// System to advance the compute clock to the next pending time, if one exists.
    pub fn advance(mut clock: ResMut<SimulationComputeClock>) {
        if clock.pending.is_empty() {
            debug!("Compute clock reached end at time {:?}", clock.now());
            clock.is_complete = true;
            return;
        }

        clock.tick();
    }
}

// TODO: This method is only in newer versions of Bevy, this is a workaround.
// https://docs.rs/bevy/latest/bevy/prelude/struct.World.html#method.spawn_at
fn spawn_at(world: &mut World, entity: Entity) {
    #[allow(deprecated)]
    let _ = world.insert_or_spawn_batch([(entity, ())]);
}
