use crate::simulation::{Simulation, SimulationState, SimulationStep};
use crate::time::SimulationTime;
use bevy::prelude::*;
use std::time::Duration;

pub struct SimulationPlaybackPlugin;

impl Plugin for SimulationPlaybackPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimulationPlaybackConfig>()
            .init_resource::<SimulationPlayback>()
            .add_event::<SetActiveSimulation>()
            .add_event::<SimulationPlaybackCommand>()
            .add_systems(
                Update,
                (activate_simulation, apply_playback_commands, advance_playback).chain(),
            );
    }
}

#[derive(Resource)]
pub struct SimulationPlaybackConfig {
    pub speed: f32,
    pub looping: LoopBehavior,
}

impl Default for SimulationPlaybackConfig {
    fn default() -> Self {
        Self {
            speed: 1.0,
            looping: LoopBehavior::None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LoopBehavior {
    None,
    Pause(Duration),
}

#[derive(Event)]
pub struct SetActiveSimulation(pub Entity);

#[derive(Event)]
pub enum SimulationPlaybackCommand {
    Play,
    Pause,
    ScrubTo(SimulationTime),
}

#[derive(Resource)]
pub struct ActiveSimulation(pub Entity);

#[derive(Resource, Default)]
pub struct SimulationPlayback {
    mode: PlaybackMode,
    elapsed: Duration,
    step_idx: usize,
}

impl SimulationPlayback {
    pub fn is_playing(&self) -> bool {
        matches!(self.mode, PlaybackMode::Playing)
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

#[derive(Default)]
enum PlaybackMode {
    #[default]
    Paused,
    Playing,
    LoopPause(Timer),
}

fn activate_simulation(world: &mut World) {
    let Some(SetActiveSimulation(entity)) = world
        .resource_mut::<Events<SetActiveSimulation>>()
        .drain()
        .last()
    else {
        return;
    };

    if world.remove_resource::<ActiveSimulation>().is_some() {
        // TODO: Clean up world state introduced by the previously active simulation.
    }

    let Some(init_step) = clone_init_step(world, entity) else {
        return;
    };
    init_step.apply(world);
    world.insert_resource(ActiveSimulation(entity));

    let mut playback = world.resource_mut::<SimulationPlayback>();
    playback.mode = PlaybackMode::Playing;
    playback.elapsed = Duration::ZERO;
    playback.step_idx = 0;
}

fn apply_playback_commands(world: &mut World) {
    let commands: Vec<SimulationPlaybackCommand> = world
        .resource_mut::<Events<SimulationPlaybackCommand>>()
        .drain()
        .collect();

    for command in commands {
        match command {
            SimulationPlaybackCommand::Play => {
                world.resource_mut::<SimulationPlayback>().mode = PlaybackMode::Playing;
            }
            SimulationPlaybackCommand::Pause => {
                world.resource_mut::<SimulationPlayback>().mode = PlaybackMode::Paused;
            }
            SimulationPlaybackCommand::ScrubTo(time) => {
                scrub_to(world, time);
            }
        }
    }
}

fn scrub_to(world: &mut World, time: SimulationTime) {
    let Some(&ActiveSimulation(entity)) = world.get_resource::<ActiveSimulation>() else {
        return;
    };
    let Some((init_step, steps)) = world.get_mut::<Simulation>(entity).map(|mut simulation| {
        let init_step = simulation.init_step().clone();
        let steps: Vec<SimulationStep> = simulation
            .steps()
            .range(..=time)
            .map(|(_, step)| step.clone())
            .collect();
        (init_step, steps)
    }) else {
        return;
    };

    let step_idx = steps.len();
    init_step.apply(world);
    for step in steps {
        step.apply(world);
    }

    let mut playback = world.resource_mut::<SimulationPlayback>();
    playback.elapsed = time.elapsed();
    playback.step_idx = step_idx;
    if matches!(playback.mode, PlaybackMode::LoopPause(_)) {
        playback.mode = PlaybackMode::Playing;
    }
}

fn advance_playback(world: &mut World) {
    let Some(&ActiveSimulation(entity)) = world.get_resource::<ActiveSimulation>() else {
        return;
    };
    let delta = world.resource::<Time>().delta();
    let config = world.resource::<SimulationPlaybackConfig>();
    let speed = config.speed;
    let looping = config.looping;

    world.resource_scope(|world, mut playback: Mut<SimulationPlayback>| {
        match &mut playback.mode {
            PlaybackMode::Paused => {}
            PlaybackMode::LoopPause(timer) => {
                timer.tick(delta);
                if timer.just_finished() {
                    if let Some(init_step) = clone_init_step(world, entity) {
                        init_step.apply(world);
                    }
                    playback.elapsed = Duration::ZERO;
                    playback.step_idx = 0;
                    playback.mode = PlaybackMode::Playing;
                }
            }
            PlaybackMode::Playing => {
                playback.elapsed += delta.mul_f32(speed);

                let Some(mut simulation) = world.get_mut::<Simulation>(entity) else {
                    return;
                };
                let complete = simulation.state() == SimulationState::Complete;
                let steps = simulation.steps();
                let num_steps = steps.len();
                let pending_steps: Vec<SimulationStep> = steps
                    .iter()
                    .skip(playback.step_idx)
                    .take_while(|(time, _)| time.elapsed() <= playback.elapsed)
                    .map(|(_, step)| step.clone())
                    .collect();
                playback.step_idx += pending_steps.len();
                for step in pending_steps {
                    step.apply(world);
                }

                if complete && playback.step_idx >= num_steps {
                    match looping {
                        LoopBehavior::None => {
                            playback.mode = PlaybackMode::Paused;
                        }
                        LoopBehavior::Pause(duration) => {
                            playback.mode =
                                PlaybackMode::LoopPause(Timer::new(duration, TimerMode::Once));
                        }
                    }
                }
            }
        }
    });
}

fn clone_init_step(world: &mut World, entity: Entity) -> Option<SimulationStep> {
    world
        .get_mut::<Simulation>(entity)
        .map(|mut simulation| simulation.init_step().clone())
}
