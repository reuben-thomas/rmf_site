use crate::*;
use rmf_site_editor::AppState;
use rmf_site_editor::bevy_egui::egui::{self, Ui};
use rmf_site_editor::occupancy::{OccupancyExporter, OccupancyInfo};
use rmf_site_editor::site::{CurrentScenario, GetModifier, Inclusion, Modifier};
use rmf_site_egui::{
    MenuEvent, MenuItem, PanelConfig, PanelSettings, PanelWidget, PanelWidgetInput, ScrollConfig,
    Tile, ToolMenu, Widget, WidgetSystem, show_panel_of_tiles,
};
use rmf_site_sim::interaction::rmf_site_egui::{
    SimulationOverviewTile, SimulationPlaybackTile, show_collapsible_section,
};

/// The allowed occupancy cell size range.
const CELL_SIZE_RANGE: std::ops::RangeInclusive<f32> = 0.01..=5.0;

/// The plugin providing an side panel to manage simulations, and a menu item to toggle its visibility.
#[derive(Default)]
pub struct SimulationUiPlugin;

impl Plugin for SimulationUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimulationPanel>()
            .add_systems(Update, SimulationPanel::handle_visibility);

        let panel_widget = PanelWidget::new(SimulationPanel::panel, app.world_mut());
        let panel = app
            .world_mut()
            .spawn((
                panel_widget,
                PanelSettings::left(),
                PanelConfig {
                    default_dimension: 300.0,
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

/// A simulation overview panel.
#[derive(Resource)]
struct SimulationPanel {
    /// Whether this panel is visible.
    show: bool,
    /// The menu item toggling this panel's visibility.
    toggle_panel: Entity,
}

impl FromWorld for SimulationPanel {
    fn from_world(world: &mut World) -> Self {
        let tool_header = world.resource::<ToolMenu>().get();
        let toggle_panel = world
            .spawn(MenuItem::Text("Simulation".into()))
            .insert(ChildOf(tool_header))
            .id();

        SimulationPanel {
            show: true,
            toggle_panel,
        }
    }
}

impl SimulationPanel {
    fn panel(In(input): In<PanelWidgetInput>, world: &mut World) {
        if *world.resource::<State<AppState>>().get() == AppState::MainMenu {
            return;
        }
        if !world.resource::<SimulationPanel>().show {
            return;
        }

        show_panel_of_tiles(In(input), world);
    }

    fn handle_visibility(
        mut menu_events: EventReader<MenuEvent>,
        mut display: ResMut<SimulationPanel>,
    ) {
        for event in menu_events.read() {
            if event.clicked() && event.source() == display.toggle_panel {
                display.show = !display.show;
            }
        }
    }
}

/// A title bar for the simulation panel.
#[derive(SystemParam)]
struct SimulationPanelHeader<'w> {
    display: ResMut<'w, SimulationPanel>,
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
    current_level: Res<'w, CurrentLevel>,
    get_inclusion_modifier: GetModifier<'w, 's, Modifier<Inclusion>>,
    tasks: Query<'w, 's, (Entity, &'static Task)>,
    grids: Query<'w, 's, (Entity, &'static Grid, &'static ChildOf)>,
    visibilities: Query<'w, 's, &'static mut Visibility>,
    occupancy: OccupancyExporter<'w, 's>,
    occupancy_info: ResMut<'w, OccupancyInfo>,
    show_occupancy_grid: Local<'s, bool>,
}

impl<'w, 's> SimulationComputeTile<'w, 's> {
    /// Entities for all direct tasks are explicitly included in the current scenario.
    fn direct_included_tasks(&self) -> Vec<Entity> {
        let Some(current_scenario_entity) = self.current_scenario.0 else {
            return Vec::default();
        };

        self.tasks
            .iter()
            .filter(|(task_entity, task)| {
                task.is_direct()
                    && self
                        .get_inclusion_modifier
                        .get(current_scenario_entity, *task_entity)
                        .map(|inclusion| **inclusion == Inclusion::Included)
                        .unwrap_or(false)
            })
            .map(|(task_entity, _)| task_entity)
            .collect()
    }

    /// Show current occupancy grid state, and controls to recalculate.
    fn show_occupancy(&mut self, ui: &mut egui::Ui) -> Option<&Grid> {
        let grid_entity = self.current_grid().map(|(entity, _)| entity);
        let mut compute = false;
        let mut cell_size = self.occupancy_info.cell_size;
        let mut show_grid = *self.show_occupancy_grid;

        ui.horizontal(|ui| {
            ui.label("Cell Size:");
            ui.add(
                egui::DragValue::new(&mut cell_size)
                    .range(CELL_SIZE_RANGE)
                    .speed(0.01)
                    .suffix(" m"),
            )
            .on_hover_text("The cell size of the next occupancy grid to be computed");
            compute = ui
                .button("Calculate")
                .on_hover_text("Calculate the occupancy grid of the current level")
                .clicked();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(grid_entity.is_some(), |ui| {
                    ui.checkbox(&mut show_grid, "Show")
                        .on_hover_text("Display the occupancy grid of the current level");
                })
                .response
                .on_disabled_hover_text("Calculate an occupancy grid to display it.");
            });
        });

        if cell_size != self.occupancy_info.cell_size {
            self.occupancy_info.cell_size = cell_size;
        }
        *self.show_occupancy_grid = show_grid;
        if compute {
            self.occupancy.calculate_and_replan();
        }

        self.set_grid_visibility(show_grid);
        self.current_grid().map(|(_, grid)| grid)
    }

    /// The occupancy grid of the current level, and the entity holding it.
    fn current_grid(&self) -> Option<(Entity, &Grid)> {
        let level = self.current_level.0?;
        self.grids
            .iter()
            .find(|(_, _, child_of)| child_of.parent() == level)
            .map(|(entity, grid, _)| (entity, grid))
    }

    /// Show or hide the occupancy grid of the current level.
    fn set_grid_visibility(&mut self, show: bool) {
        let Some((entity, _)) = self.current_grid() else {
            return;
        };
        let Ok(mut visibility) = self.visibilities.get_mut(entity) else {
            return;
        };

        visibility.set_if_neq(if show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
}

impl<'w, 's> WidgetSystem<Tile> for SimulationComputeTile<'w, 's> {
    fn show(_: Tile, ui: &mut Ui, state: &mut SystemState<Self>, world: &mut World) {
        let mut params = state.get_mut(world);
        let tasks = params.direct_included_tasks();
        let mut compute = None;

        show_collapsible_section(ui, "Compute", |ui| {
            ui.label(format!("Direct Tasks: {}", tasks.len()));

            let has_occupancy = params.show_occupancy(ui).is_some();

            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut *params.new_simulation_name);

                let has_any_task = !tasks.is_empty();
                let has_name = !params.new_simulation_name.trim().is_empty();

                ui.add_enabled_ui(has_any_task && has_name && has_occupancy, |ui| {
                    if ui.button("Compute").clicked() {
                        compute = Some(params.new_simulation_name.trim().to_string());
                    }
                })
                .response
                .on_disabled_hover_text({
                    let mut reasons = Vec::new();
                    if !has_any_task {
                        reasons.push("Add at least one direct task included in this scenario.");
                    }
                    if !has_name {
                        reasons.push("Specify a non-empty name for this simulation.");
                    }
                    if !has_occupancy {
                        reasons.push("Compute the occupancy grid of the current level.");
                    }
                    reasons.join("\n")
                });
            });
        });

        if let Some(name) = compute {
            crate::simulation::spawn_simulation(world, &tasks, name);
        }
    }
}
