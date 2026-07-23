//! Playback of computed [`Simulation`] steps in the main world.

use crate::simulation::{Simulation, SimulationComputeState};
use crate::sync::SimulationState;
use crate::time::SimulationTime;
use bevy::ecs::event::EventCursor;
use bevy::prelude::*;
use std::time::Duration;

/// Plugin that applies computed [`Simulation`] steps to the main world over time.
pub struct SimulationPlaybackPlugin;

impl Plugin for SimulationPlaybackPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SimulationPlaybackCommand>()
            .init_resource::<SimulationPlayback>()
            .add_systems(
                Update,
                (
                    process_commands,
                    execute_playback,
                    run_active_visualization_schedule,
                )
                    .chain(),
            );
    }
}

/// Commands for controlling playback of an active Simulation.
#[derive(Event, Clone, Copy, Debug)]
pub enum SimulationPlaybackCommand {
    /// Selects the simulation entity whose steps are played back, or deselects with `None`.
    SetActiveSimulation(Option<Entity>),
    /// Plays from the current playback time at the specified speed multiplier.
    Play { speed: f32 },
    /// Pauses at the current playback time.
    Pause,
    /// Seeks to the given target, forwards or backwards.
    Seek(SimulationPlaybackSeek),
    /// Seeks to the start of the simulation and pauses.
    SeekToStart,
    /// Seeks to the last computed step of the simulation and pauses.
    SeekToEnd,
    /// Sets the behaviour of playback upon reaching the end of the active simulation.
    SetEndBehaviour(SimulationPlaybackEndBehaviour),
}

/// A seek target for [`SimulationPlaybackCommand::Seek`].
#[derive(Clone, Copy, Debug)]
pub enum SimulationPlaybackSeek {
    /// Seek to the very first step at or after the specified time.
    Time { time: SimulationTime },
    /// Seek so that exactly `applied_steps` steps have been applied.
    Step { applied_steps: usize },
    /// Seek by the given number of steps in the specified direction.
    StepDelta {
        steps: usize,
        direction: SeekDirection,
    },
    /// Seek to the very first step at or after the time after the addition of the duration offset.
    TimeDelta {
        duration: Duration,
        direction: SeekDirection,
    },
    /// Seek to the last computed step.
    End,
}

// TODO: Distinguish OOB between a complete and in progress computation simulation.
/// The requested seek falls out of simulation's computed steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimulationPlaybackSeekOutOfBounds;

/// The direction to seek in [`SimulationPlaybackSeek`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekDirection {
    Advance,
    Revert,
}

// TODO: Handle distinguishing a fully computed sim vs an in-progress sim.
/// Behaviour of playback upon reaching the end of a simulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SimulationPlaybackEndBehaviour {
    /// Pause at the end of the simulation.
    #[default]
    Pause,
    /// Pause for the specified duration, then replay from the initial state.
    ReplayAfterPause(Duration),
}

#[derive(Clone, Debug, Default)]
pub enum SimulationPlaybackState {
    #[default]
    Paused,
    Playing {
        speed: f32,
    },
    PendingReplay {
        timer: Timer,
        speed: f32,
    },
}

#[derive(Resource, Default)]
pub struct SimulationPlayback(Option<SimulationActivePlayback>);

impl SimulationPlayback {
    pub fn state(&self) -> Option<&SimulationPlaybackState> {
        self.0.as_ref().map(|active| &active.state)
    }

    pub fn time(&self) -> Option<SimulationTime> {
        self.0.as_ref().map(|active| active.time)
    }

    /// The entity of the currently active simulation, if any.
    pub fn active_simulation(&self) -> Option<Entity> {
        self.0.as_ref().map(|active| active.simulation)
    }

    fn set_active_simulation(&mut self, world: &mut World, simulation: Option<Entity>) {
        if let Some(existing) = self.0.take() {
            existing.deactivate(world);
        }

        self.0 = simulation.map(|entity| SimulationActivePlayback::activate(world, entity));
    }
}

struct SimulationActivePlayback {
    /// The entity associated with the selected [`Simulation`] for playback.
    simulation: Entity,
    /// A snapshot of the world's state before this playback, to restore the state of the main world upon terminating
    /// this playback.
    pre_simulation_state: SimulationState,
    /// The state of playback.
    state: SimulationPlaybackState,
    /// The current time in the Simulation.
    ///
    /// Unlike computation, this time is progressed
    /// incrementally rather than as discrete steps associated with events.
    time: SimulationTime,
    /// The number of [`SimulationStep`]s that have been applied so far.
    applied_steps: usize,
    /// The behaviour upon playing up to the end of the simulation.
    end_behaviour: SimulationPlaybackEndBehaviour,
}

impl SimulationActivePlayback {
    fn activate(world: &mut World, simulation: Entity) -> Self {
        let sim = world
            .get::<Simulation>(simulation)
            .expect("Active simulation entity has no Simulation component");
        let pre_simulation_state = SimulationState::extract(sim.synchronizer(), world);

        let mut active = Self {
            simulation,
            pre_simulation_state,
            state: SimulationPlaybackState::default(),
            time: SimulationTime::default(),
            applied_steps: 0,
            end_behaviour: SimulationPlaybackEndBehaviour::default(),
        };
        active.reset(world);
        active
    }

    fn deactivate(self, world: &mut World) {
        let synchronizer = world
            .get::<Simulation>(self.simulation)
            .expect("Active simulation entity has no Simulation component")
            .synchronizer()
            .clone();
        synchronizer.sync(&self.pre_simulation_state.0, world);
    }

    fn reset(&mut self, world: &mut World) {
        let sim = world
            .entity_mut(self.simulation)
            .take::<Simulation>()
            .expect("Active simulation entity has no Simulation component");
        sim.synchronizer().sync(&sim.init_state().0, world);
        world.entity_mut(self.simulation).insert(sim);
        self.time = SimulationTime::from(Duration::ZERO);
        self.applied_steps = 0;
    }

    /// Applies exactly up to `applied_steps`.
    fn apply_up_to(&mut self, world: &mut World, applied_steps: usize) {
        if applied_steps < self.applied_steps {
            self.reset(world);
        }

        let simulation = self.simulation_mut(world);
        let pending_steps: Vec<_> = simulation
            .steps()
            .iter()
            .take(applied_steps)
            .skip(self.applied_steps)
            .map(|(time, step)| (*time, step.clone()))
            .collect();

        for (time, step) in pending_steps {
            step.apply(world);
            self.time = time;
            self.applied_steps += 1;
        }
    }

    /// Seeks so that exactly `applied_steps` steps have been applied.
    fn seek_to_step(
        &mut self,
        world: &mut World,
        applied_steps: usize,
    ) -> Result<(), SimulationPlaybackSeekOutOfBounds> {
        let available = self.simulation_mut(world).steps().len();
        if applied_steps > available {
            return Err(SimulationPlaybackSeekOutOfBounds);
        }

        self.apply_up_to(world, applied_steps);
        Ok(())
    }

    /// Seeks to the specified time by applying all steps occuring up to and inclusive of `time`.
    fn seek_to_time(&mut self, world: &mut World, time: SimulationTime) {
        let applied_steps = self.simulation_mut(world).steps().range(..=time).count();
        self.apply_up_to(world, applied_steps);
        // Time is progressed incrementally, rather than last the time of the last step.
        self.time = time;
    }

    // TODO: Distinguish between available end and actual end.
    /// Seeks so that every computed step has been applied.
    fn seek_to_end(&mut self, world: &mut World) {
        let available = self.simulation_mut(world).steps().len();
        self.apply_up_to(world, available);
    }

    /// Seeks the playback to a `target`.
    fn seek(
        &mut self,
        world: &mut World,
        target: SimulationPlaybackSeek,
    ) -> Result<(), SimulationPlaybackSeekOutOfBounds> {
        match target {
            SimulationPlaybackSeek::Time { time } => {
                self.seek_to_time(world, time);
            }
            SimulationPlaybackSeek::Step { applied_steps } => {
                self.seek_to_step(world, applied_steps)?;
            }
            SimulationPlaybackSeek::StepDelta { steps, direction } => {
                let target = match direction {
                    SeekDirection::Advance => self.applied_steps.saturating_add(steps),
                    SeekDirection::Revert => self
                        .applied_steps
                        .checked_sub(steps)
                        .ok_or(SimulationPlaybackSeekOutOfBounds)?,
                };
                self.seek_to_step(world, target)?;
            }
            SimulationPlaybackSeek::TimeDelta {
                duration,
                direction,
            } => {
                let elapsed = match direction {
                    SeekDirection::Advance => self.time.elapsed() + duration,
                    SeekDirection::Revert => self
                        .time
                        .elapsed()
                        .checked_sub(duration)
                        .ok_or(SimulationPlaybackSeekOutOfBounds)?,
                };
                self.seek_to_time(world, SimulationTime::new(elapsed));
            }
            SimulationPlaybackSeek::End => {
                self.seek_to_end(world);
            }
        }
        Ok(())
    }

    /// Whether playback is at the end of a simulation that has finished computation.
    fn at_end(&self, world: &mut World) -> bool {
        let simulation = self.simulation_mut(world);
        matches!(
            simulation.state(),
            SimulationComputeState::Complete | SimulationComputeState::Failed
        ) && self.applied_steps >= simulation.steps().len()
    }

    fn simulation_mut<'w>(&self, world: &'w mut World) -> &'w mut Simulation {
        world
            .get_mut::<Simulation>(self.simulation)
            .expect("Active simulation entity has no Simulation component")
            .into_inner()
    }
}

/// Processes [`SimulationPlaybackCommand`]s.
fn process_commands(world: &mut World, mut cursor: Local<EventCursor<SimulationPlaybackCommand>>) {
    let commands: Vec<_> = cursor
        .read(world.resource::<Events<SimulationPlaybackCommand>>())
        .copied()
        .collect();
    if commands.is_empty() {
        return;
    }

    world.resource_scope(|world, mut playback: Mut<SimulationPlayback>| {
        for command in commands {
            if let SimulationPlaybackCommand::SetActiveSimulation(simulation) = command {
                playback.set_active_simulation(world, simulation);
                continue;
            }
            let Some(active) = playback.0.as_mut() else {
                warn!("Ignoring playback command with no active simulation: {command:?}");
                continue;
            };
            match command {
                SimulationPlaybackCommand::SetActiveSimulation(_) => {}
                SimulationPlaybackCommand::Play { speed } => {
                    active.state = SimulationPlaybackState::Playing { speed };
                }
                SimulationPlaybackCommand::Pause => {
                    active.state = SimulationPlaybackState::Paused;
                }
                SimulationPlaybackCommand::Seek(target) => {
                    if let Err(err) = active.seek(world, target) {
                        warn!("Ignoring out-of-bounds playback seek {target:?}: {err:?}");
                    }
                }
                SimulationPlaybackCommand::SeekToStart => {
                    if let Err(err) = active.seek(
                        world,
                        SimulationPlaybackSeek::Time {
                            time: SimulationTime::default(),
                        },
                    ) {
                        warn!("Failed to seek playback to start: {err:?}");
                    }
                    active.state = SimulationPlaybackState::Paused;
                }
                SimulationPlaybackCommand::SeekToEnd => {
                    if let Err(err) = active.seek(world, SimulationPlaybackSeek::End) {
                        warn!("Failed to seek playback to end: {err:?}");
                    }
                    active.state = SimulationPlaybackState::Paused;
                }
                SimulationPlaybackCommand::SetEndBehaviour(end_behaviour) => {
                    active.end_behaviour = end_behaviour;
                }
            }
        }
    });
}

/// Executes playing and end behaviour.
fn execute_playback(world: &mut World) {
    let delta = world.resource::<Time>().delta();

    world.resource_scope(|world, mut playback: Mut<SimulationPlayback>| {
        let Some(active) = playback.0.as_mut() else {
            return;
        };
        match &mut active.state {
            SimulationPlaybackState::Paused => {}
            SimulationPlaybackState::Playing { speed } => {
                let speed = *speed;
                let target_time = SimulationTime::new(active.time.elapsed() + delta.mul_f32(speed));
                active.seek_to_time(world, target_time);

                if active.at_end(world) {
                    match active.end_behaviour {
                        SimulationPlaybackEndBehaviour::Pause => {
                            active.state = SimulationPlaybackState::Paused;
                        }
                        SimulationPlaybackEndBehaviour::ReplayAfterPause(pause_duration) => {
                            active.state = SimulationPlaybackState::PendingReplay {
                                timer: Timer::new(pause_duration, TimerMode::Once),
                                speed,
                            };
                        }
                    }
                }
            }
            SimulationPlaybackState::PendingReplay { timer, speed } => {
                let speed = *speed;
                timer.tick(delta);
                if timer.just_finished() {
                    active.reset(world);
                    active.state = SimulationPlaybackState::Playing { speed };
                }
            }
        }
    });
}

fn run_active_visualization_schedule(world: &mut World) {
    let Some(simulation) = world.resource::<SimulationPlayback>().active_simulation() else {
        return;
    };

    let mut sim = world
        .entity_mut(simulation)
        .take::<Simulation>()
        .expect("Active simulation entity has no Simulation component");
    sim.run_visualization_schedule(world);
    world.entity_mut(simulation).insert(sim);
}
