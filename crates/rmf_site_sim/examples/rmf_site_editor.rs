use bevy::{
    ecs::system::{Command, SystemParam, SystemState},
    prelude::*,
};
use rmf_site_editor::bevy_egui::egui::{self, Ui};
use rmf_site_editor::site::{
    CurrentScenario, GetModifier, Inclusion, Modifier, NameInSite, Task, TaskParams,
};
use rmf_site_editor::{AppState, SiteEditor};
use rmf_site_egui::{
    MenuEvent, MenuItem, PanelConfig, PanelSettings, PanelWidget, PanelWidgetInput, ScrollConfig,
    Tile, ToolMenu, Widget, WidgetSystem, show_panel_of_tiles,
};
use rmf_site_sim::compute::SimulationComputeClock;
use rmf_site_sim::event::DiscreteChangeWriter;
use rmf_site_sim::playback::{
    SimulationActivePlaybackView, SimulationPlaybackCommand, SimulationPlaybackPlugin,
    SimulationPlaybackView, SimulationReplayBehaviour,
};
use rmf_site_sim::playback_ui::{
    SimulationPlaybackEventTable, SimulationPlaybackMenu, SimulationPlaybackTimeline,
};
use rmf_site_sim::time::SimulationTime;
use rmf_site_sim::{Simulation, SimulationBuilder, SimulationComputeState, SimulationPlugin};
use std::time::Duration;

fn main() {
    App::new()
        .add_plugins((SiteEditor::default(), DiscreteEventSimulationPlugin))
        .run();
}

#[derive(Default)]
struct DiscreteEventSimulationPlugin;

impl Plugin for DiscreteEventSimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((SimulationPlugin, SimulationPlaybackPlugin))
            .init_resource::<SimulationPanelToggle>()
            .init_resource::<SimulationPanelMenuItem>()
            .add_systems(Update, SimulationPanelMenuItem::handle_visibility);

        let panel_widget = PanelWidget::new(SimulationPanelToggle::panel, app.world_mut());
        let panel = app
            .world_mut()
            .spawn((
                panel_widget,
                PanelSettings::left(),
                PanelConfig {
                    default_dimension: 320.0,
                    horizontal_scrolling: ScrollConfig {
                        enable_scroll: false,
                        auto_shrink: false,
                    },
                    ..Default::default()
                },
            ))
            .id();

        for widget in [
            Widget::<Tile>::new::<SimulationPanelHeader>(app.world_mut()),
            Widget::<Tile>::new::<SimulationComputeTile>(app.world_mut()),
            Widget::<Tile>::new::<SimulationsTile>(app.world_mut()),
            Widget::<Tile>::new::<SimulationPlaybackTile>(app.world_mut()),
        ] {
            app.world_mut().spawn(widget).insert(ChildOf(panel));
        }
    }
}

/// Manage visibility for the SimulationPanel.
#[derive(Resource)]
struct SimulationPanelToggle {
    show: bool,
}

impl Default for SimulationPanelToggle {
    fn default() -> Self {
        Self { show: true }
    }
}

impl SimulationPanelToggle {
    fn panel(In(input): In<PanelWidgetInput>, world: &mut World) {
        if *world.resource::<State<AppState>>().get() == AppState::MainMenu {
            return;
        }
        if !world.resource::<SimulationPanelToggle>().show {
            return;
        }

        show_panel_of_tiles(In(input), world);
    }
}

/// Menu item to toggle visibility for the simulation panel.
#[derive(Resource)]
struct SimulationPanelMenuItem {
    toggle_panel: Entity,
}

impl FromWorld for SimulationPanelMenuItem {
    fn from_world(world: &mut World) -> Self {
        let tool_header = world.resource::<ToolMenu>().get();
        let toggle_panel = world
            .spawn(MenuItem::Text("Simulation".into()))
            .insert(ChildOf(tool_header))
            .id();

        SimulationPanelMenuItem { toggle_panel }
    }
}

impl SimulationPanelMenuItem {
    fn handle_visibility(
        mut menu_events: EventReader<MenuEvent>,
        simulation_panel_menu: Res<SimulationPanelMenuItem>,
        mut display: ResMut<SimulationPanelToggle>,
    ) {
        for event in menu_events.read() {
            if event.clicked() && event.source() == simulation_panel_menu.toggle_panel {
                display.show = !display.show;
            }
        }
    }
}

/// A title bar for the simulation panel.
#[derive(SystemParam)]
struct SimulationPanelHeader<'w> {
    display: ResMut<'w, SimulationPanelToggle>,
}

impl<'w> WidgetSystem<Tile> for SimulationPanelHeader<'w> {
    fn show(_: Tile, ui: &mut Ui, state: &mut SystemState<Self>, world: &mut World) {
        let mut params = state.get_mut(world);

        ui.horizontal(|ui| {
            ui.heading("Discrete Event Simulation");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("❌").clicked() {
                    params.display.show = false;
                }
            });
        });
        ui.separator();
    }
}

/// A tile for computing a new simulation from the current scenario.
#[derive(SystemParam)]
struct SimulationComputeTile<'w, 's> {
    new_simulation_name: Local<'s, String>,
    current_scenario: Res<'w, CurrentScenario>,
    get_inclusion_modifier: GetModifier<'w, 's, Modifier<Inclusion>>,
    tasks: Query<'w, 's, (Entity, &'static Task)>,
}

impl<'w, 's> SimulationComputeTile<'w, 's> {
    /// Entities for all direct tasks are either implicitly inherited or included in this scenario.
    fn direct_included_or_inherited_tasks(&self) -> Vec<Entity> {
        let Some(current_scenario_entity) = self.current_scenario.0 else {
            return Vec::default();
        };

        return self
            .tasks
            .iter()
            .filter(|(task_entity, task)| {
                task.is_direct()
                    && self
                        .get_inclusion_modifier
                        .get(current_scenario_entity, *task_entity)
                        .map(|inclusion| **inclusion == Inclusion::Included)
                        .unwrap_or(true)
            })
            .map(|(task_entity, _)| task_entity)
            .collect();
    }
}

impl<'w, 's> WidgetSystem<Tile> for SimulationComputeTile<'w, 's> {
    fn show(_: Tile, ui: &mut Ui, state: &mut SystemState<Self>, world: &mut World) {
        let mut params = state.get_mut(world);
        let tasks = params.direct_included_or_inherited_tasks();

        show_collapsible_section(ui, "Compute", |ui| {
            ui.label(format!("Direct Tasks: {}", tasks.len()));
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut *params.new_simulation_name);

                let has_any_task = !tasks.is_empty();
                let has_name = !params.new_simulation_name.trim().is_empty();

                ui.add_enabled_ui(has_any_task && has_name, |ui| {
                    if ui.button("Compute").clicked() {
                        simulation::spawn_simulation(
                            world,
                            &tasks,
                            params.new_simulation_name.trim().to_string(),
                        );
                    }
                })
                .response
                .on_disabled_hover_text({
                    let mut reasons = Vec::new();
                    if !has_any_task {
                        reasons.push(
                            "Add at least one direct task included or inherited in this scenario.",
                        );
                    }
                    if !has_name {
                        reasons.push("Specify a non-empty name for this simulation.");
                    }
                    reasons.join("\n")
                });
            });
        });
    }
}

/// Lists all available simulations.
#[derive(SystemParam)]
struct SimulationsTile<'w, 's> {
    simulations: Query<'w, 's, (Entity, &'static NameInSite, &'static Simulation)>,
}

impl<'w, 's> SimulationsTile<'w, 's> {
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
        let event_count: usize = steps.values().map(|step| step.events.len()).sum();

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

impl<'w, 's> WidgetSystem<Tile> for SimulationsTile<'w, 's> {
    fn show(_: Tile, ui: &mut Ui, state: &mut SystemState<Self>, world: &mut World) {
        let mut params = state.get_mut(world);
        show_collapsible_section(ui, "Simulations", |ui| {
            params.show_simulations(ui);
        });
    }
}

/// Selects which computed simulation is being played back and drives it.
#[derive(SystemParam)]
struct SimulationPlaybackTile<'w, 's> {
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

mod simulation {
    use crate::*;

    #[derive(Component, Clone, Copy)]
    pub struct IncludedSimulationTask;

    #[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
    pub enum RequestStatus {
        Pending,
        Arrived,
    }

    #[derive(Clone)]
    pub struct RequestArrival {
        pub task: Entity,
    }

    impl Command for RequestArrival {
        fn apply(self, world: &mut World) {
            world.entity_mut(self.task).insert(RequestStatus::Arrived);
        }
    }

    pub fn spawn_simulation(
        world: &mut World,
        tasks: &[Entity],
        name: String,
    ) -> Entity {
        for &entity in tasks {
            world
                .entity_mut(entity)
                .insert((IncludedSimulationTask, RequestStatus::Pending));
        }

        let simulation = SimulationBuilder::<IncludedSimulationTask>::new()
            .register_component::<Task>()
            .register_component::<TaskParams>()
            .register_component::<RequestStatus>()
            .add_simulation_systems(request_generator)
            .build(world);

        for &entity in tasks {
            world.entity_mut(entity).remove::<IncludedSimulationTask>();
        }

        let simulation_entity = world.spawn(simulation).id();
        world.entity_mut(simulation_entity).insert(NameInSite(name));
        simulation_entity
    }

    pub fn request_generator(
        tasks: Query<(Entity, &TaskParams, &RequestStatus), With<Task>>,
        clock: Res<SimulationComputeClock>,
        mut changes: DiscreteChangeWriter,
    ) {
        let now = clock.now();
        for (task, params, status) in tasks.iter() {
            if *status != RequestStatus::Pending {
                continue;
            }
            let request_secs = params.request_time().unwrap_or(0).max(0) as u64;
            let time = SimulationTime::new(Duration::from_secs(request_secs)).max(now);
            changes.write(time, RequestArrival { task });
        }
    }
}

/// Create a collapsible section with contents.
fn show_collapsible_section(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
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
