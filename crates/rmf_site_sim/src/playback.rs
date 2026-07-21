//! Playback of computed [`Simulation`] steps in the main world.

use crate::simulation::{Simulation, SimulationState};
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
            .add_systems(Update, (process_commands, execute_playback).chain());
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
    /// Sets the behaviour of playback upon reaching the end of the active simulation.
    SetEndBehaviour(SimulationPlaybackEndBehaviour),
}

/// A seek target for [`SimulationPlaybackCommand::Seek`].
#[derive(Clone, Copy, Debug)]
pub enum SimulationPlaybackSeek {
    /// Seek to the very first step at or after the specified time.
    Time { time: SimulationTime },
    /// Seek so that exactly `step_idx` steps have been applied.
    Step { step_idx: usize },
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
}

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
pub struct SimulationPlayback(Option<SimulationAcyivePlayback>);

impl SimulationPlayback {
    pub fn state(&self) -> Option<&SimulationPlaybackState> {
        self.0.as_ref().map(|active| &active.state)
    }

    fn set_active_simulation(&mut self, world: &mut World, simulation: Option<Entity>) {
        if let Some(_existing_simulation) = self.0.as_ref() {
            todo!("Handle cleanup of simulation artefacts");
        }

        self.0 = simulation.map(SimulationAcyivePlayback::new);
        if let Some(active) = self.0.as_mut() {
            active.reset(world);
        }
    }
}

struct SimulationAcyivePlayback {
    /// The entity associated with the selected [`Simulation`] for playback.
    simulation: Entity,
    /// The state of playback.
    state: SimulationPlaybackState,
    /// The current time in the Simulation.
    ///
    /// Unlike computation, this time is progressed
    /// incrementally rather than as discrete steps associated with events.
    time: SimulationTime,
    /// The last applied [`SimulationStep`] index.
    next_step_idx: usize,
    /// The behaviour upon playing up to the end of the simulation.
    end_behaviour: SimulationPlaybackEndBehaviour,
}

impl SimulationAcyivePlayback {
    fn new(simulation: Entity) -> Self {
        Self {
            simulation,
            state: SimulationPlaybackState::default(),
            time: SimulationTime::default(),
            next_step_idx: 0,
            end_behaviour: SimulationPlaybackEndBehaviour::default(),
        }
    }

    // Reset playback to initial state.
    fn reset(&mut self, world: &mut World) {
        let simulation = self.simulation_mut(world);
        simulation.init_step().clone().apply(world);
        self.time = SimulationTime::from(Duration::ZERO);
        self.next_step_idx = 0;
    }

    /// Seeks by applying up to, and inclusive of `step_idx`.
    fn seek_to_step(&mut self, world: &mut World, step_idx: usize) {
        if step_idx < self.next_step_idx {
            self.reset(world);
        }

        let simulation = self.simulation_mut(world);
        let pending_steps: Vec<_> = simulation
            .steps()
            .iter()
            .take(step_idx + 1)
            .skip(self.next_step_idx)
            .map(|(time, step)| (*time, step.clone()))
            .collect();

        for (time, step) in pending_steps {
            step.apply(world);
            self.time = time;
            self.next_step_idx += 1;
        }
    }

    /// Seeks to the specified time by applying all steps occuring up to and inclusive of `time`.
    fn seek_to_time(&mut self, world: &mut World, time: SimulationTime) {
        let step_idx = self.simulation_mut(world).steps().range(..=time).count();
        self.seek_to_step(world, step_idx);
        // Time is progressed incrementally, rather than last the time of the last step.
        self.time = time;
    }

    /// Seeks the playback to a `target`.
    fn seek(&mut self, world: &mut World, target: SimulationPlaybackSeek) {
        // TODO: Handle overflows, out of bounds
        match target {
            SimulationPlaybackSeek::Time { time } => {
                self.seek_to_time(world, time);
            }
            SimulationPlaybackSeek::Step { step_idx } => {
                self.seek_to_step(world, step_idx);
            }
            SimulationPlaybackSeek::StepDelta { steps, direction } => {
                self.seek_to_step(
                    world,
                    match direction {
                        SeekDirection::Advance => self.next_step_idx + steps,
                        SeekDirection::Revert => self.next_step_idx - (steps + 1),
                    },
                );
            }
            SimulationPlaybackSeek::TimeDelta {
                duration,
                direction,
            } => {
                self.seek_to_time(
                    world,
                    SimulationTime::new(match direction {
                        SeekDirection::Advance => self.time.elapsed() + duration,
                        SeekDirection::Revert => self.time.elapsed() - duration,
                    }),
                );
            }
        }
    }

    /// Whether playback is at the end of a simulation that has finished computation.
    fn at_end(&self, world: &mut World) -> bool {
        let simulation = self.simulation_mut(world);
        matches!(
            simulation.state(),
            SimulationState::Complete | SimulationState::Failed
        ) && self.next_step_idx >= simulation.steps().len()
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
                    active.seek(world, target);
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
