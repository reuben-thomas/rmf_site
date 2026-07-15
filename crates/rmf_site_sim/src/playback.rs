use crate::simulation::Simulation;
use bevy::prelude::*;

const STEP_DURATION_SECONDS: f32 = 0.25;
const PAUSE_BEFORE_REPLAY_SECONDS: f32 = 2.0;

pub struct SimulationPlaybackPlugin;

impl Plugin for SimulationPlaybackPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<PlaybackState>()
            .add_systems(Update, play.run_if(in_state(PlaybackState::Playing)))
            .add_systems(Update, pause.run_if(in_state(PlaybackState::Paused)))
            .add_systems(OnEnter(PlaybackState::Paused), start_pause)
            .add_systems(OnExit(PlaybackState::Paused), reset);
    }
}

#[derive(States, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum PlaybackState {
    #[default]
    Playing,
    Paused,
}

struct Play {
    timer: Timer,
    step_idx: usize,
}

impl Default for Play {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(STEP_DURATION_SECONDS, TimerMode::Repeating),
            step_idx: 0,
        }
    }
}

#[derive(Resource)]
struct PauseTimer(Timer);

fn play(world: &mut World, mut state: Local<Play>) {
    state.timer.tick(world.resource::<Time>().delta());
    if !state.timer.just_finished() {
        return;
    }

    // TODO: Handle multiple simulations
    let mut query = world.query::<&mut Simulation>();
    let mut simulation = query.single_mut(world).unwrap();
    let events = simulation
        .steps()
        .values()
        .nth(state.step_idx)
        .map(|step| step.events.clone());
    match events {
        Some(events) => {
            state.step_idx += 1;
            for event in events {
                event.apply(world);
            }
        }
        None => {
            state.step_idx = 0;
            world
                .resource_mut::<NextState<PlaybackState>>()
                .set(PlaybackState::Paused);
        }
    }
}

fn start_pause(mut commands: Commands) {
    commands.insert_resource(PauseTimer(Timer::from_seconds(
        PAUSE_BEFORE_REPLAY_SECONDS,
        TimerMode::Once,
    )));
}

fn pause(
    time: Res<Time>,
    mut timer: ResMut<PauseTimer>,
    mut next_state: ResMut<NextState<PlaybackState>>,
) {
    timer.0.tick(time.delta());
    if timer.0.just_finished() {
        next_state.set(PlaybackState::Playing);
    }
}

fn reset(world: &mut World) {
    world.remove_resource::<PauseTimer>();
    let mut query = world.query::<&mut Simulation>();
    let mut simulation = query.single_mut(world).unwrap();
    let events = simulation.init_step().events.clone();
    for event in events {
        event.apply(world);
    }
}
