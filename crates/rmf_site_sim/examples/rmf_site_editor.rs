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
use rmf_site_sim::event::CandidateEventWriter;
use rmf_site_sim::interaction::rmf_site_egui::{
    SimulationOverviewTile, SimulationPlaybackTile, show_collapsible_section,
};
use rmf_site_sim::playback::SimulationPlaybackPlugin;
use rmf_site_sim::time::SimulationTime;
use rmf_site_sim::{SimulationBuilder, SimulationPlugin};
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
            Widget::<Tile>::new::<SimulationOverviewTile>(app.world_mut()),
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
    /// Entities for all direct tasks are either implicitly inherited or
    /// included in this scenario.
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

mod simulation {
    use crate::*;

    #[derive(Component, Clone, Copy)]
    pub struct IncludedSimulationTask;

    #[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
    pub enum RequestStatus {
        Pending,
        Arrived,
    }

    #[derive(Clone, Debug)]
    pub struct RequestArrival {
        pub task: Entity,
    }

    impl Command for RequestArrival {
        fn apply(self, world: &mut World) {
            world.entity_mut(self.task).insert(RequestStatus::Arrived);
        }
    }

    pub fn spawn_simulation(world: &mut World, tasks: &[Entity], name: String) -> Entity {
        for &entity in tasks {
            world
                .entity_mut(entity)
                .insert((IncludedSimulationTask, RequestStatus::Pending));
        }

        let simulation = SimulationBuilder::<IncludedSimulationTask>::new()
            .register_component::<Task>()
            .register_component::<TaskParams>()
            .register_component::<RequestStatus>()
            .add_prediction_systems(request_generator)
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
        mut changes: CandidateEventWriter,
    ) {
        let now = clock.now();
        for (task, params, status) in tasks.iter() {
            if *status != RequestStatus::Pending {
                continue;
            }
            let request_secs = params.request_time().unwrap_or(0).max(0) as u64;
            let time = SimulationTime::new(Duration::from_secs(request_secs)).max(now);
            changes.predict(time, RequestArrival { task });
        }
    }
}
