//! Playback of computed [`Simulation`] steps in the main world.

use crate::simulation::{Simulation, SimulationComputeState, SimulationState};
use crate::sync::Synchronizer;
use crate::time::SimulationTime;
use bevy::ecs::event::EventCursor;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::ops::RangeInclusive;
use std::time::Duration;

/// Plugin that applies computed [`Simulation`] steps to the main world over
/// time.
pub struct SimulationPlaybackPlugin;

impl Plugin for SimulationPlaybackPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SimulationPlaybackCommand>()
            .init_resource::<SimulationPlayback>()
            .add_systems(
                Update,
                (
                    SimulationPlayback::process_commands,
                    SimulationPlayback::execute_playback,
                    SimulationPlayback::visualize,
                )
                    .chain(),
            );
    }
}

// TODO(@reuben-thomas): Should something similar be configurable at runtime?
// We may have high or low frequency event simulations.
// Another option would be constraining the range to maximum events / second, or maximum events / frame of 1 based on the active simulation.
/// The allowed playback speed range.
pub const PLAYBACK_SPEED_RANGE: RangeInclusive<f32> = 0.01..=100.0;

/// Commands for controlling playback of an active Simulation.
#[derive(Event, Clone, Copy, Debug)]
pub enum SimulationPlaybackCommand {
    /// Selects a Simulation for playback.
    SetActiveSimulation(Option<Entity>),
    /// Plays from the current playback time, with the behaviour at end defined
    /// by [`SimulationReplayBehaviour`].
    Play,
    /// Pauses at the current playback time.
    Pause,
    /// Plays if playback is paused, and pauses otherwise.
    TogglePlayPause,
    /// Sets the speed multiplier of playback, clamped to
    /// [`PLAYBACK_SPEED_RANGE`].
    SetSpeed(f32),
    /// Seeks to the given target.
    Seek(SimulationPlaybackSeek),
    /// Seeks to the start of the simulation and pauses.
    SeekToStart,
    /// Seeks to the last computed step of the simulation and pauses.
    SeekToLast,
    /// Sets the behaviour of playback upon reaching the end of the active
    /// simulation.
    SetReplayBehaviour(SimulationReplayBehaviour),
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
    /// Seek to the very first step at or after the time after the addition of
    /// the duration offset.
    TimeDelta {
        duration: Duration,
        direction: SeekDirection,
    },
    /// Seek to the last computed step.
    End,
}

/// The direction to seek in [`SimulationPlaybackSeek`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekDirection {
    Advance,
    Revert,
}

// TODO(@reuben-thomas): Distinguish OOB between a complete and in progress
// computation simulation.
/// The requested seek falls out of simulation's computed steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimulationPlaybackSeekOutOfBounds;

/// Behaviour of replay upon reaching the end of a simulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimulationReplayBehaviour(pub Option<Duration>);

/// The play/pause state of the active playback.
#[derive(Clone, Debug, Default)]
pub enum SimulationPlaybackState {
    #[default]
    Paused,
    Playing,
    PendingReplay {
        timer: Timer,
    },
}

/// The active playback, if any.
#[derive(Resource, Default)]
pub struct SimulationPlayback(Option<SimulationActivePlayback>);

impl SimulationPlayback {
    /// The entity of the simulation currently being played back, if any.
    pub fn active_simulation(&self) -> Option<Entity> {
        self.0.as_ref().map(|active| active.simulation)
    }

    /// Set a new active simulation.
    fn set_active_simulation(&mut self, world: &mut World, simulation: Option<Entity>) {
        if let Some(existing) = self.0.take() {
            existing.deactivate(world);
        }

        self.0 = simulation.and_then(|entity| SimulationActivePlayback::activate(world, entity));
    }

    /// Processes [`SimulationPlaybackCommand`]s.
    fn process_commands(
        world: &mut World,
        mut cursor: Local<EventCursor<SimulationPlaybackCommand>>,
    ) {
        let commands: Vec<_> = cursor
            .read(world.resource::<Events<SimulationPlaybackCommand>>())
            .copied()
            .collect();
        if commands.is_empty() {
            return;
        }

        world.resource_scope(|world, mut playback: Mut<SimulationPlayback>| {
            for command in commands {
                match command {
                    SimulationPlaybackCommand::SetActiveSimulation(simulation) => {
                        playback.set_active_simulation(world, simulation);
                    }
                    command => {
                        let Some(active) = playback.0.as_mut() else {
                            warn!(
                                "Ignoring playback command with no active simulation: {command:?}"
                            );
                            continue;
                        };
                        active.execute_command(world, command);
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
                SimulationPlaybackState::Playing => {
                    active.seek_to_time(
                        world,
                        SimulationTime::new(active.time.elapsed() + delta.mul_f32(active.speed)),
                    );

                    if active.at_end(active.simulation_ref(world)) {
                        active.state = match active.replay_behaviour.0 {
                            Some(pause) => SimulationPlaybackState::PendingReplay {
                                timer: Timer::new(pause, TimerMode::Once),
                            },
                            None => SimulationPlaybackState::Paused,
                        };
                    }
                }
                SimulationPlaybackState::PendingReplay { timer } => {
                    timer.tick(delta);
                    if timer.just_finished() {
                        active.load_start(world);
                        active.state = SimulationPlaybackState::Playing;
                    }
                }
            }
        });
    }

    // TODO(@reuben-thomas): Is there a better pattern here to give temporary
    // access?
    /// Runs the active simulation's visualization schedule on the main world.
    fn visualize(world: &mut World) {
        let Some(simulation_entity) = world.resource::<SimulationPlayback>().active_simulation()
        else {
            return;
        };
        let Some(mut simulation) = world.get_mut::<Simulation>(simulation_entity) else {
            return;
        };

        let mut schedule = simulation.take_visualization_schedule();
        schedule.run(world);
        if let Some(mut simulation) = world.get_mut::<Simulation>(simulation_entity) {
            simulation.restore_visualization_schedule(schedule);
        }
    }
}

// TODO(@reuben-thomas): Use getters since these fields should not be directly
// mutated.
pub struct SimulationActivePlayback {
    /// The entity associated with the [`Simulation`] being played back.
    simulation: Entity,
    /// A snapshot of the world's state before this playback, to restore the
    /// state of the main world upon terminating this playback.
    pre_simulation_state: SimulationState,
    // TODO(@reuben-thomas): Clarify the use of a synchronizer for this purpose, can we assume the
    // only relevant changes are guaranteed to be of the same component type?
    /// A synchronizer used to capture the state of the world before playback,
    /// and after.
    synchronizer: Synchronizer,
    /// The play/pause state of playback.
    pub state: SimulationPlaybackState,
    /// The current time in the simulation, progressed incrementally rather than
    /// as discrete steps associated with events.
    pub time: SimulationTime,
    /// The number of steps that have been applied so far.
    pub applied_steps: usize,
    /// The speed multiplier applied while playing.
    pub speed: f32,
    /// The behaviour upon playing up to the end of the simulation.
    pub replay_behaviour: SimulationReplayBehaviour,
}

impl SimulationActivePlayback {
    /// The entity associated with the [`Simulation`] being played back.
    pub fn simulation_entity(&self) -> Entity {
        self.simulation
    }

    /// Activate a new simulation.
    ///
    /// The current state of the world is saved, and the initial state of the
    /// simulation is loaded.
    fn activate(world: &mut World, simulation: Entity) -> Option<Self> {
        let Some(sim) = world.get::<Simulation>(simulation) else {
            error!(
                "Cannot activate playback for {simulation:?}: entity has no Simulation component"
            );
            return None;
        };

        let mut active_playback = Self {
            simulation,
            pre_simulation_state: SimulationState::extract(sim.synchronizer(), world),
            synchronizer: sim.synchronizer().clone(),
            state: SimulationPlaybackState::default(),
            time: SimulationTime::default(),
            applied_steps: 0,
            speed: 1.0,
            replay_behaviour: SimulationReplayBehaviour::default(),
        };
        active_playback.load_start(world);
        Some(active_playback)
    }

    // TODO(@reuben-thomas): Override `std::ops::Drop` to prevent users from
    // forgetting to call deactivate and reset the state of the world?
    /// Restores the world to the state before activation of the current
    /// simulation for playback
    fn deactivate(self, world: &mut World) {
        self.synchronizer.sync(&self.pre_simulation_state.0, world);
    }

    /// Whether playback is either playing, or waiting for replay.
    pub fn is_playing(&self) -> bool {
        !matches!(self.state, SimulationPlaybackState::Paused)
    }

    /// Whether at the start of a simulation, and no steps have been applied.
    pub fn at_start(&self) -> bool {
        self.applied_steps == 0
    }

    /// Whether at the last available computed step, when computation of the
    /// simulation has yet to complete.
    pub fn at_last_available(&self, simulation: &Simulation) -> bool {
        self.applied_steps >= simulation.steps().len()
    }

    /// Whether at the end of a simulation that has finished computation, and
    /// all available steps have been applied.
    pub fn at_end(&self, simulation: &Simulation) -> bool {
        simulation.state() != SimulationComputeState::Computing
            && self.at_last_available(simulation)
    }

    fn execute_command(&mut self, world: &mut World, command: SimulationPlaybackCommand) {
        match command {
            SimulationPlaybackCommand::Play => {
                self.play(world);
            }
            SimulationPlaybackCommand::Pause => {
                self.state = SimulationPlaybackState::Paused;
            }
            SimulationPlaybackCommand::TogglePlayPause => {
                if self.is_playing() {
                    self.state = SimulationPlaybackState::Paused;
                } else {
                    self.play(world);
                }
            }
            SimulationPlaybackCommand::SetSpeed(speed) => {
                self.speed =
                    speed.clamp(*PLAYBACK_SPEED_RANGE.start(), *PLAYBACK_SPEED_RANGE.end());
            }
            SimulationPlaybackCommand::Seek(target) => {
                if let Err(err) = self.seek(world, target) {
                    warn!("Ignoring out-of-bounds playback seek {target:?}: {err:?}");
                }
            }
            SimulationPlaybackCommand::SeekToStart => {
                self.seek_to_time(world, SimulationTime::default());
                self.state = SimulationPlaybackState::Paused;
            }
            SimulationPlaybackCommand::SeekToLast => {
                self.seek_to_end(world);
                self.state = SimulationPlaybackState::Paused;
            }
            SimulationPlaybackCommand::SetReplayBehaviour(replay_behaviour) => {
                self.replay_behaviour = replay_behaviour;
            }
            SimulationPlaybackCommand::SetActiveSimulation(_) => {
                unreachable!("Should be handled by `SimulationPlayback`")
            }
        }
    }

    /// Loads the simulation's start state to the world.
    fn load_start(&mut self, world: &mut World) {
        let sim = world
            .entity_mut(self.simulation)
            .take::<Simulation>()
            .expect("Active simulation entity has no Simulation component");

        self.synchronizer.sync(&sim.init_state().0, world);
        world.entity_mut(self.simulation).insert(sim);
        self.time = SimulationTime::default();
        self.applied_steps = 0;
    }

    /// Applies exactly up to `applied_steps`.
    fn apply_up_to(&mut self, world: &mut World, applied_steps: usize) {
        if applied_steps < self.applied_steps {
            self.load_start(world);
        }

        let pending_steps: Vec<_> = self
            .simulation_ref(world)
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
        if applied_steps > self.simulation_ref(world).steps().len() {
            return Err(SimulationPlaybackSeekOutOfBounds);
        }

        self.apply_up_to(world, applied_steps);
        Ok(())
    }

    /// Seeks to the specified time by applying all steps occuring up to and
    /// inclusive of `time`.
    fn seek_to_time(&mut self, world: &mut World, time: SimulationTime) {
        let applied_steps = self.simulation_ref(world).steps().range(..=time).count();
        self.apply_up_to(world, applied_steps);
        // Time is progressed incrementally, rather than last the time of the last step.
        self.time = time;
    }

    // TODO(@reuben-thomas): Distinguish between available end and actual end.
    /// Seeks so that every computed step has been applied.
    fn seek_to_end(&mut self, world: &mut World) {
        let available = self.simulation_ref(world).steps().len();
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

    /// Sets playback to play, restarting from the beginning if it has already
    /// reached the end.
    fn play(&mut self, world: &mut World) {
        if self.at_end(self.simulation_ref(world)) {
            self.load_start(world);
        }
        self.state = SimulationPlaybackState::Playing;
    }

    fn simulation_ref<'w>(&self, world: &'w World) -> &'w Simulation {
        world
            .get::<Simulation>(self.simulation)
            .expect("Active simulation entity has no Simulation component")
    }
}

/// A helper [`SystemParam`] to view the active playback and its corresponding
/// simulation.
#[derive(SystemParam)]
pub struct SimulationPlaybackView<'w, 's> {
    playback: Res<'w, SimulationPlayback>,
    simulations: Query<'w, 's, &'static Simulation>,
}

impl SimulationPlaybackView<'_, '_> {
    pub fn active(&self) -> Option<SimulationActivePlaybackView<'_>> {
        let playback = self.playback.0.as_ref()?;
        let simulation = self.simulations.get(playback.simulation).ok()?;
        Some(SimulationActivePlaybackView {
            playback,
            simulation,
        })
    }
}

/// The active playback and its corresponding simulation.
#[derive(Clone, Copy)]
pub struct SimulationActivePlaybackView<'a> {
    pub playback: &'a SimulationActivePlayback,
    pub simulation: &'a Simulation,
}

impl SimulationActivePlaybackView<'_> {
    /// Whether at the last available computed step, when computation of the
    /// simulation has yet to complete.
    pub fn at_last_available(&self) -> bool {
        self.playback.at_last_available(self.simulation)
    }

    /// Whether at the end of a simulation that has finished computation, and
    /// all available steps have been applied.
    pub fn at_end(&self) -> bool {
        self.playback.at_end(self.simulation)
    }
}
