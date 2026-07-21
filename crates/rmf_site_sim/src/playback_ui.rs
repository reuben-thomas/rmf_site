use crate::playback::{
    SeekDirection, SimulationPlayback, SimulationPlaybackCommand, SimulationPlaybackSeek,
    SimulationPlaybackState,
};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// The minimum playback speed reachable via [`SimulationPlaybackKeyboardAction::ChangeSpeed`].
const MIN_PLAYBACK_SPEED: f32 = 0.05;

pub struct SimulationPlaybackKeyboardPlugin;

impl Plugin for SimulationPlaybackKeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimulationPlaybackKeymap>()
            .init_resource::<SimulationPlaybackSpeed>()
            .add_systems(Update, playback_keyboard_controls);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SimulationPlaybackKeyboardAction {
    TogglePlayPause,
    ChangeSpeed {
        delta: f32,
    },
    Step {
        steps: usize,
        direction: SeekDirection,
    },
    SeekToStart,
    SeekToEnd,
}

#[derive(Resource, Clone, Debug, Deref, DerefMut)]
pub struct SimulationPlaybackKeymap(pub HashMap<KeyCode, SimulationPlaybackKeyboardAction>);

impl Default for SimulationPlaybackKeymap {
    fn default() -> Self {
        Self(HashMap::from_iter([
            (
                KeyCode::Space,
                SimulationPlaybackKeyboardAction::TogglePlayPause,
            ),
            (
                KeyCode::Equal,
                SimulationPlaybackKeyboardAction::ChangeSpeed { delta: 0.5 },
            ),
            (
                KeyCode::Minus,
                SimulationPlaybackKeyboardAction::ChangeSpeed { delta: -0.5 },
            ),
            (
                KeyCode::ArrowRight,
                SimulationPlaybackKeyboardAction::Step {
                    steps: 1,
                    direction: SeekDirection::Advance,
                },
            ),
            (
                KeyCode::ArrowLeft,
                SimulationPlaybackKeyboardAction::Step {
                    steps: 1,
                    direction: SeekDirection::Revert,
                },
            ),
            (KeyCode::Home, SimulationPlaybackKeyboardAction::SeekToStart),
            (KeyCode::End, SimulationPlaybackKeyboardAction::SeekToEnd),
        ]))
    }
}

#[derive(Resource, Clone, Copy, Debug, Deref, DerefMut)]
pub struct SimulationPlaybackSpeed(pub f32);

impl Default for SimulationPlaybackSpeed {
    fn default() -> Self {
        Self(1.0)
    }
}

fn playback_keyboard_controls(
    keys: Res<ButtonInput<KeyCode>>,
    keymap: Res<SimulationPlaybackKeymap>,
    playback: Res<SimulationPlayback>,
    mut speed: ResMut<SimulationPlaybackSpeed>,
    mut commands: EventWriter<SimulationPlaybackCommand>,
) {
    for (&key, &action) in keymap.iter() {
        if !keys.just_pressed(key) {
            continue;
        }
        match action {
            SimulationPlaybackKeyboardAction::TogglePlayPause => match playback.state() {
                Some(SimulationPlaybackState::Playing { .. }) => {
                    commands.write(SimulationPlaybackCommand::Pause);
                }
                _ => {
                    commands.write(SimulationPlaybackCommand::Play { speed: speed.0 });
                }
            },
            SimulationPlaybackKeyboardAction::ChangeSpeed { delta } => {
                speed.0 = (speed.0 + delta).max(MIN_PLAYBACK_SPEED);
                if matches!(
                    playback.state(),
                    Some(SimulationPlaybackState::Playing { .. })
                ) {
                    commands.write(SimulationPlaybackCommand::Play { speed: speed.0 });
                }
            }
            SimulationPlaybackKeyboardAction::Step { steps, direction } => {
                commands.write(SimulationPlaybackCommand::Seek(
                    SimulationPlaybackSeek::StepDelta { steps, direction },
                ));
            }
            SimulationPlaybackKeyboardAction::SeekToStart => {
                commands.write(SimulationPlaybackCommand::SeekToStart);
            }
            SimulationPlaybackKeyboardAction::SeekToEnd => {
                commands.write(SimulationPlaybackCommand::SeekToEnd);
            }
        }
    }
}
