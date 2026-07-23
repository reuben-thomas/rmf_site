use crate::event::CandidateDiscreteEvents;
use crate::schedule::{SimulationStartup, SimulationSystemExec};
use crate::simulation::{
    SimulationComputeState, SimulationComputeUpdate, SimulationPluginFactory, SimulationState,
};
use crate::sync::{SimulationEventBuffer, StateUpdateSender, Synchronizer};
use crate::time::SimulationTime;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, TaskPool};
use crossbeam_channel::Sender;
use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Computes [`SimulationStep`](crate::simulation::SimulationStep)s for a simulation in a
/// [`AsyncComputeTaskPool`].
pub fn compute_async(
    init_state: SimulationState,
    startup_schedule: Schedule,
    system_schedule: Schedule,
    plugins: Vec<SimulationPluginFactory>,
    synchronizer: Synchronizer,
    sender: Sender<SimulationComputeUpdate>,
) {
    let completion_sender = sender.clone();
    AsyncComputeTaskPool::get_or_init(TaskPool::new)
        .spawn(async move {
            let result = catch_unwind(AssertUnwindSafe(move || {
                let mut app = build_app(
                    init_state,
                    startup_schedule,
                    system_schedule,
                    plugins,
                    synchronizer,
                    sender,
                );
                app.run();
            }));
            let state = match result {
                Ok(()) => SimulationComputeState::Complete,
                Err(_) => SimulationComputeState::Failed,
            };
            let _ = completion_sender.send(SimulationComputeUpdate::State(state));
        })
        .detach();
}

/// Builds an [`App`] with the initial state, plugins, schedules, and sets [`compute_simulation`]
/// as the runner.
fn build_app(
    init_state: SimulationState,
    startup_schedule: Schedule,
    system_schedule: Schedule,
    plugins: Vec<SimulationPluginFactory>,
    synchronizer: Synchronizer,
    sender: Sender<SimulationComputeUpdate>,
) -> App {
    let mut app = App::new();
    app.add_schedule(startup_schedule);
    app.add_schedule(system_schedule);
    app.init_resource::<SimulationComputeClock>()
        .init_resource::<CandidateDiscreteEvents>()
        .init_resource::<SimulationEventBuffer>()
        .insert_resource(StateUpdateSender::new(sender));
    app.set_runner(compute_simulation);
    synchronizer.sync(&init_state.0, app.world_mut());
    for plugin in plugins {
        plugin(&mut app);
    }
    app
}
/// A runner to compute and send one [`crate::simulation::SimulationStep`] at a time to the main app.
fn compute_simulation(mut app: App) -> AppExit {
    let world = app.world_mut();
    world.run_schedule(SimulationStartup);
    loop {
        compute_time_step(world);
        let _ = world.run_system_cached(SimulationEventBuffer::send_step);
        let _ = world.run_system_cached(CandidateDiscreteEvents::update_clock_with_pending_time);
        let _ = world.run_system_cached(SimulationComputeClock::advance);

        // TODO:
        // Configurable end conditions could include:
        // - A maximum number of steps:
        // - A generic user implemented trait e.g. `crate::simulation::EndCondition`
        // - A user system that sends an event
        if world.resource::<SimulationComputeClock>().at_end {
            break;
        }
    }
    AppExit::Success
}

fn compute_time_step(world: &mut World) {
    let _ =
        world.run_system_cached(CandidateDiscreteEvents::execute_highest_priority_current_event);
    loop {
        world.run_schedule(SimulationSystemExec);
        let executed = world
            .run_system_cached(CandidateDiscreteEvents::execute_highest_priority_current_event)
            .unwrap();
        if !executed {
            break;
        }
    }
}

/// Clock for tracking the current simulation time being computed, as well as pending times to be processed.
#[derive(Resource, Default)]
pub struct SimulationComputeClock {
    current: SimulationTime,
    pending: BTreeSet<SimulationTime>,
    at_end: bool,
}

impl SimulationComputeClock {
    /// The current simulation time.
    pub fn now(&self) -> SimulationTime {
        self.current
    }

    /// Adds a pending time to be processed.
    ///
    /// Returns whether a new pending time was added.
    pub fn insert_pending(&mut self, time: SimulationTime) -> bool {
        if time <= self.current {
            panic!(
                "Tried to add time {time:?} that is not greater than the current time {:?}.",
                self.now()
            )
        }
        self.pending.insert(time)
    }

    /// System to advance the compute clock to the next pending time, if one exists.
    fn advance(mut clock: ResMut<SimulationComputeClock>) {
        if clock.at_end {
            return;
        }

        match clock.pending.pop_first() {
            Some(time) => clock.current = time,
            None => {
                clock.at_end = true;
                debug!("Compute clock reached end at time {:?}", clock.now());
            }
        }
    }
}
