use crate::simulation::Simulation;
use bevy::prelude::*;
use std::time::Duration;

const PLAYBACK_SPEED: f32 = 5.0;
const PAUSE_BEFORE_REPLAY_SECONDS: f32 = 1.0;

pub struct SimulationPlaybackPlugin;

impl Plugin for SimulationPlaybackPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<PlaybackState>()
            .init_resource::<Play>()
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

#[derive(Resource, Default)]
struct Play {
    elapsed: Duration,
    step_idx: usize,
}

fn play(world: &mut World) {
    world.resource_scope(|world, mut play: Mut<Play>| {
        play.elapsed += world.resource::<Time>().delta().mul_f32(PLAYBACK_SPEED);

        // TODO: Handle multiple simulations
        let mut query = world.query::<&mut Simulation>();
        let mut simulation = query.single_mut(world).unwrap();
        let steps = simulation.steps();

        if play.step_idx >= steps.len() {
            play.step_idx = 0;
            play.elapsed = Duration::ZERO;
            world
                .resource_mut::<NextState<PlaybackState>>()
                .set(PlaybackState::Paused);
            return;
        }

        let step_events: Vec<_> = steps
            .iter()
            .skip(play.step_idx)
            .take_while(|(time, _)| time.elapsed() <= play.elapsed)
            .map(|(_, step)| step.events.clone())
            .collect();
        play.step_idx += step_events.len();
        for events in step_events {
            for event in events {
                event.apply(world);
            }
        }
    });
}

#[derive(Resource)]
struct Pause(Timer);

fn start_pause(mut commands: Commands) {
    commands.insert_resource(Pause(Timer::from_seconds(
        PAUSE_BEFORE_REPLAY_SECONDS,
        TimerMode::Once,
    )));
}

fn pause(
    time: Res<Time>,
    mut timer: ResMut<Pause>,
    mut next_state: ResMut<NextState<PlaybackState>>,
) {
    timer.0.tick(time.delta());
    if timer.0.just_finished() {
        next_state.set(PlaybackState::Playing);
    }
}

fn reset(world: &mut World) {
    world.remove_resource::<Pause>();
    let mut query = world.query::<&mut Simulation>();
    let mut simulation = query.single_mut(world).unwrap();
    let events = simulation.init_step().events.clone();
    for event in events {
        event.apply(world);
    }
}
