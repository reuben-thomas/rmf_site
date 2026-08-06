//! Keyboard shortcuts for controlling simulation playback.

use crate::playback::{
    SeekDirection, SimulationPlaybackCommand, SimulationPlaybackSeek, SimulationPlaybackView,
};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

pub struct SimulationPlaybackKeyboardPlugin;

impl Plugin for SimulationPlaybackKeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimulationPlaybackKeymap>()
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

fn playback_keyboard_controls(
    keys: Res<ButtonInput<KeyCode>>,
    keymap: Res<SimulationPlaybackKeymap>,
    playback: SimulationPlaybackView,
    mut commands: EventWriter<SimulationPlaybackCommand>,
) {
    let Some(active) = playback.active() else {
        return;
    };

    for (&key, &action) in keymap.iter() {
        if !keys.just_pressed(key) {
            continue;
        }
        match action {
            SimulationPlaybackKeyboardAction::TogglePlayPause => {
                commands.write(SimulationPlaybackCommand::TogglePlayPause);
            }
            SimulationPlaybackKeyboardAction::ChangeSpeed { delta } => {
                commands.write(SimulationPlaybackCommand::SetSpeed(
                    active.playback.speed + delta,
                ));
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
                commands.write(SimulationPlaybackCommand::SeekToLast);
            }
        }
    }
}
