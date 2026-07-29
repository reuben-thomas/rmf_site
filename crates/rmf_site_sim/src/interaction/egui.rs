use crate::event::DynDiscreteEvent;
use crate::playback::{
    PLAYBACK_SPEED_RANGE, SeekDirection, SimulationActivePlaybackView, SimulationPlaybackCommand,
    SimulationPlaybackSeek,
};
use crate::time::SimulationTime;
use bevy::prelude::*;
use bevy_egui::egui;
use std::hash::Hash;
use std::time::Duration;

/// A menu to control play/pause, seeking to the next or previous event, or seeking to the start or end of the simulation.
pub struct SimulationPlaybackMenu<'a> {
    view: SimulationActivePlaybackView<'a>,
}

impl<'a> SimulationPlaybackMenu<'a> {
    pub fn new(view: SimulationActivePlaybackView<'a>) -> Self {
        Self { view }
    }

    pub fn show(self, ui: &mut egui::Ui, commands: &mut EventWriter<SimulationPlaybackCommand>) {
        let view = self.view;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!view.playback.at_start(), egui::Button::new("Start"))
                .on_hover_text("Seek to start")
                .clicked()
            {
                commands.write(SimulationPlaybackCommand::SeekToStart);
            }
            if ui
                .add_enabled(!view.playback.at_start(), egui::Button::new("Previous"))
                .on_hover_text("Step backwards")
                .clicked()
            {
                commands.write(SimulationPlaybackCommand::Seek(
                    SimulationPlaybackSeek::StepDelta {
                        steps: 1,
                        direction: SeekDirection::Revert,
                    },
                ));
            }
            if ui
                .button(if view.playback.is_playing() {
                    "Pause"
                } else {
                    "Play"
                })
                .clicked()
            {
                commands.write(SimulationPlaybackCommand::TogglePlayPause);
            }
            if ui
                .add_enabled(!view.at_last_available(), egui::Button::new("Next"))
                .on_hover_text("Step forwards")
                .clicked()
            {
                commands.write(SimulationPlaybackCommand::Seek(
                    SimulationPlaybackSeek::StepDelta {
                        steps: 1,
                        direction: SeekDirection::Advance,
                    },
                ));
            }
            if ui
                .add_enabled(!view.at_last_available(), egui::Button::new("End"))
                .on_hover_text("Seek to end")
                .clicked()
            {
                commands.write(SimulationPlaybackCommand::SeekToLast);
            }

            let mut speed = view.playback.speed;
            let speed_changed = ui
                .add(
                    egui::DragValue::new(&mut speed)
                        .range(PLAYBACK_SPEED_RANGE)
                        .speed(0.1)
                        .suffix("x"),
                )
                .on_hover_text("Playback speed")
                .changed();
            if speed_changed {
                commands.write(SimulationPlaybackCommand::SetSpeed(speed));
            }
        });
    }
}

/// A scrubbable slider spanning the computed duration of the simulation.
pub struct SimulationPlaybackTimeline<'a> {
    view: SimulationActivePlaybackView<'a>,
}

impl<'a> SimulationPlaybackTimeline<'a> {
    pub fn new(view: SimulationActivePlaybackView<'a>) -> Self {
        Self { view }
    }

    /// Shows the slider, writing a seek to `commands` when the user scrubs it.
    pub fn show(self, ui: &mut egui::Ui, commands: &mut EventWriter<SimulationPlaybackCommand>) {
        let end_secs = self.view.simulation.duration().as_secs_f64();
        let mut changed_secs = self.view.playback.time.elapsed().as_secs_f64();
        let mut changed = false;

        ui.scope(|ui| {
            ui.spacing_mut().slider_width = ui.available_width();
            changed = ui
                .add(
                    egui::Slider::new(&mut changed_secs, 0.0..=end_secs)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed();
        });

        if changed {
            commands.write(SimulationPlaybackCommand::Seek(
                SimulationPlaybackSeek::Time {
                    time: SimulationTime::new(Duration::from_secs_f64(changed_secs)),
                },
            ));
        }
    }
}

pub struct SimulationPlaybackEventTable<'a> {
    view: SimulationActivePlaybackView<'a>,
    max_height: f32,
}

impl<'a> SimulationPlaybackEventTable<'a> {
    pub fn new(view: SimulationActivePlaybackView<'a>) -> Self {
        Self {
            view,
            max_height: 300.0,
        }
    }

    pub fn max_height(mut self, max_height: f32) -> Self {
        self.max_height = max_height;
        self
    }

    pub fn show(self, ui: &mut egui::Ui, commands: &mut EventWriter<SimulationPlaybackCommand>) {
        let steps = self.view.simulation.steps();
        if steps.is_empty() {
            ui.label("No steps computed yet.");
            return;
        }

        let applied_steps = self.view.playback.applied_steps;

        egui::ScrollArea::vertical()
            .id_salt("simulation_playback_event_table")
            .max_height(self.max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("simulation_playback_event_table_grid")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Time");
                        ui.strong("Step");
                        ui.end_row();

                        for (index, (time, step)) in steps.iter().enumerate() {
                            let is_current = applied_steps > 0 && index == applied_steps - 1;

                            let response = ui.selectable_label(
                                is_current,
                                egui::RichText::new(format!("{:?}", time.elapsed())).monospace(),
                            );
                            if response.clicked() {
                                commands.write(SimulationPlaybackCommand::Seek(
                                    SimulationPlaybackSeek::Step {
                                        applied_steps: index + 1,
                                    },
                                ));
                            }
                            if is_current {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }

                            let event_count = step.event_count();
                            if event_count == 1 {
                                // A lone event stands in for the step, without a grouping header.
                                if let Some(event) = step.events().next() {
                                    show_event(ui, index, event);
                                }
                            } else {
                                egui::CollapsingHeader::new(format!("{event_count} Events"))
                                    .id_salt(index)
                                    .show(ui, |ui| {
                                        for (event_index, event) in step.events().enumerate() {
                                            show_event(ui, (index, event_index), event);
                                        }
                                    });
                            }
                            ui.end_row();
                        }
                    });
            });
    }
}

/// Shows one event as a collapsible header naming its kind, revealing the
/// values it carries.
fn show_event(ui: &mut egui::Ui, id_salt: impl Hash, event: &dyn DynDiscreteEvent) {
    egui::CollapsingHeader::new(event.name())
        .id_salt(id_salt)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(format!("{event:#?}")).monospace());
        });
}
