use crate::interaction::egui::{
    SimulationPlaybackEventTable, SimulationPlaybackMenu, SimulationPlaybackTimeline,
};
use crate::playback::{
    SimulationActivePlaybackView, SimulationPlaybackCommand, SimulationPlaybackView,
    SimulationReplayBehaviour,
};
use crate::simulation::{Simulation, SimulationComputeState};
use bevy::ecs::system::SystemParam;
use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use bevy_egui::egui::{self, Ui};
use rmf_site_egui::{Tile, WidgetSystem};
use rmf_site_format::NameInSite;
use std::time::Duration;

/// Lists all available simulations.
#[derive(SystemParam)]
pub struct SimulationOverviewTile<'w, 's> {
    simulations: Query<'w, 's, (Entity, &'static NameInSite, &'static Simulation)>,
}

impl<'w, 's> SimulationOverviewTile<'w, 's> {
    fn show_simulations(&mut self, ui: &mut Ui) {
        if self.simulations.is_empty() {
            ui.label("No ");
            return;
        }

        show_card(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("simulation_cards")
                .max_height(200.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (entity, name, simulation) in self.simulations.iter() {
                        let (state, color) = match simulation.state() {
                            SimulationComputeState::Computing => {
                                ("Computing", egui::Color32::YELLOW)
                            }
                            SimulationComputeState::Complete => ("Computed", egui::Color32::GREEN),
                            SimulationComputeState::Failed => ("Failed", egui::Color32::RED),
                        };

                        ui.push_id(entity, |ui| {
                            show_card(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.strong(name.as_str());
                                    ui.colored_label(color, state);
                                });
                                ui.collapsing("Details", |ui| {
                                    Self::show_simulation_details(ui, simulation);
                                });
                            });
                        });
                    }
                });
        });
    }

    fn show_simulation_details(ui: &mut Ui, simulation: &Simulation) {
        let steps = simulation.steps();
        let event_count: usize = steps.values().map(|step| step.event_count()).sum();

        ui.label(format!(
            "Extracted entities: {}",
            simulation.init_state().0.entities().len()
        ));
        ui.label(format!("Steps: {}", steps.len()));
        ui.label(format!("Events: {event_count}"));
        if !steps.is_empty() {
            ui.label(format!("Duration: {:?}", simulation.duration()));
        }
    }
}

impl<'w, 's> WidgetSystem<Tile> for SimulationOverviewTile<'w, 's> {
    fn show(_: Tile, ui: &mut Ui, state: &mut SystemState<Self>, world: &mut World) {
        let mut params = state.get_mut(world);
        show_collapsible_section(ui, "Simulations", |ui| {
            params.show_simulations(ui);
        });
    }
}

/// Selects which computed simulation is being played back and drives it.
#[derive(SystemParam)]
pub struct SimulationPlaybackTile<'w, 's> {
    playback: SimulationPlaybackView<'w, 's>,
    playback_commands: EventWriter<'w, SimulationPlaybackCommand>,
    simulations: Query<'w, 's, (Entity, &'static NameInSite, &'static Simulation)>,
}

impl<'w, 's> SimulationPlaybackTile<'w, 's> {
    fn show_playback(&mut self, ui: &mut Ui) {
        if self.simulations.is_empty() {
            ui.label("Compute a simulation to enable playback.");
            return;
        }

        let active_simulation = self
            .playback
            .active()
            .map(|active| active.playback.simulation_entity());

        ui.horizontal(|ui| {
            ui.label("Simulation:");
            let selected_name = active_simulation
                .and_then(|entity| self.simulations.get(entity).ok())
                .map(|(_, name, _)| name.to_string())
                .unwrap_or_else(|| "Select a simulation...".to_string());

            let mut selection = active_simulation;
            egui::ComboBox::from_id_salt("playback_simulation")
                .selected_text(selected_name)
                .show_ui(ui, |ui| {
                    for (entity, name, _) in self.simulations.iter() {
                        ui.selectable_value(&mut selection, Some(entity), name.as_str());
                    }
                });
            if selection != active_simulation {
                self.playback_commands
                    .write(SimulationPlaybackCommand::SetActiveSimulation(selection));
            }

            ui.add_enabled_ui(active_simulation.is_some(), |ui| {
                if ui.button("❌").on_hover_text("Unload").clicked() {
                    self.playback_commands
                        .write(SimulationPlaybackCommand::SetActiveSimulation(None));
                }
            });
        });

        let Self {
            playback,
            playback_commands,
            ..
        } = self;
        let Some(active) = playback.active() else {
            return;
        };

        ui.add_space(4.0);
        SimulationPlaybackMenu::new(active).show(ui, playback_commands);
        SimulationPlaybackTimeline::new(active).show(ui, playback_commands);
        Self::show_loop_settings(ui, active, playback_commands);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let current = active.playback.time.elapsed();
            let total = active.simulation.duration();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.monospace(format!("{:?}/{:?}", current.min(total), total));
            });
        });
        ui.add_space(4.0);
        Self::show_event_table(ui, active, playback_commands);
    }

    fn show_loop_settings(
        ui: &mut Ui,
        active: SimulationActivePlaybackView,
        commands: &mut EventWriter<SimulationPlaybackCommand>,
    ) {
        let SimulationReplayBehaviour(pause) = active.playback.replay_behaviour;
        let mut replay = pause.is_some();
        let mut pause_secs = pause.unwrap_or_default().as_secs_f32();

        let mut changed = false;
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut replay, "Loop on Completion").changed();
            if replay {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut pause_secs)
                            .range(0.0_f32..=10.0)
                            .speed(0.1)
                            .suffix("s"),
                    )
                    .on_hover_text("Pause before replaying")
                    .changed();
            }
        });

        if changed {
            commands.write(SimulationPlaybackCommand::SetReplayBehaviour(
                SimulationReplayBehaviour(replay.then(|| Duration::from_secs_f32(pause_secs))),
            ));
        }
    }

    fn show_event_table(
        ui: &mut Ui,
        active: SimulationActivePlaybackView,
        commands: &mut EventWriter<SimulationPlaybackCommand>,
    ) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            SimulationPlaybackEventTable::new(active).show(ui, commands)
        });
    }
}

impl<'w, 's> WidgetSystem<Tile> for SimulationPlaybackTile<'w, 's> {
    fn show(_: Tile, ui: &mut Ui, state: &mut SystemState<Self>, world: &mut World) {
        let mut params = state.get_mut(world);
        show_collapsible_section(ui, "Playback", |ui| {
            params.show_playback(ui);
        });
    }
}

/// Create a collapsible section with contents.
pub fn show_collapsible_section(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(title)
        .default_open(true)
        .show(ui, add_contents);
    ui.separator();
}

/// Create a card spanning all available width.
fn show_card(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::default()
        .inner_margin(4.0)
        .corner_radius(2.0)
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::GRAY))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui);
        });
}
