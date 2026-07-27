use bevy::{
    ecs::system::{SystemParam, SystemState},
    prelude::*,
};
use rmf_site_editor::bevy_egui::egui::{self, Grid, Ui};
use rmf_site_editor::site::{CurrentScenario, GetModifier, Inclusion, Modifier, Task};
use rmf_site_editor::{AppState, SiteEditor};
use rmf_site_egui::{
    MenuEvent, MenuItem, PanelWidget, PanelWidgetInput, ToolMenu, TryShowWidgetWorld, Widget,
    WidgetSystem,
};

fn main() {
    App::new()
        .add_plugins((SiteEditor::default(), DiscreteEventSimulationPlugin))
        .run();
}

#[derive(Default)]
struct DiscreteEventSimulationPlugin;

impl Plugin for DiscreteEventSimulationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimulationPanelDisplay>()
            .init_resource::<SimulationPanelMenu>()
            .add_systems(Update, SimulationPanelMenu::handle_visibility);

        let panel = PanelWidget::new(SimulationPanelDisplay::panel, app.world_mut());
        let widget = Widget::new::<SimulationPanelWidget>(app.world_mut());
        app.world_mut().spawn((panel, widget));
    }
}

#[derive(Resource)]
struct SimulationPanelDisplay {
    show: bool,
}

impl Default for SimulationPanelDisplay {
    fn default() -> Self {
        Self { show: true }
    }
}

impl SimulationPanelDisplay {
    fn panel(In(input): In<PanelWidgetInput>, world: &mut World) {
        if *world.resource::<State<AppState>>().get() == AppState::MainMenu {
            return;
        }
        if !world.resource::<SimulationPanelDisplay>().show {
            return;
        }

        egui::SidePanel::left("simulation_panel")
            .resizable(true)
            .min_width(320.0)
            .show(&input.context, |ui| {
                if let Err(err) = world.try_show(input.id, ui) {
                    error!("Unable to display simulation panel: {err:?}");
                }
            });
    }
}

#[derive(Resource)]
struct SimulationPanelMenu {
    toggle_panel: Entity,
}

impl FromWorld for SimulationPanelMenu {
    fn from_world(world: &mut World) -> Self {
        let tool_header = world.resource::<ToolMenu>().get();
        let toggle_panel = world
            .spawn(MenuItem::Text("Simulation".into()))
            .insert(ChildOf(tool_header))
            .id();

        SimulationPanelMenu { toggle_panel }
    }
}

impl SimulationPanelMenu {
    fn handle_visibility(
        mut menu_events: EventReader<MenuEvent>,
        simulation_panel_menu: Res<SimulationPanelMenu>,
        mut display: ResMut<SimulationPanelDisplay>,
    ) {
        for event in menu_events.read() {
            if event.clicked() && event.source() == simulation_panel_menu.toggle_panel {
                display.show = !display.show;
            }
        }
    }
}

#[derive(SystemParam)]
struct SimulationPanelWidget<'w, 's> {
    display: ResMut<'w, SimulationPanelDisplay>,
    current_scenario: Res<'w, CurrentScenario>,
    get_inclusion_modifier: GetModifier<'w, 's, Modifier<Inclusion>>,
    tasks: Query<'w, 's, (Entity, &'static Task)>,
}

impl<'w, 's> WidgetSystem for SimulationPanelWidget<'w, 's> {
    fn show(_: (), ui: &mut Ui, state: &mut SystemState<Self>, world: &mut World) {
        let mut params = state.get_mut(world);

        ui.heading("Discrete Event Simulation");
        ui.separator();

        let Some(current_scenario_entity) = params.current_scenario.0 else {
            ui.label("No scenario selected.");
            if ui.button("Close").clicked() {
                params.display.show = false;
            }
            return;
        };

        let included_direct_tasks: Vec<(Entity, Task)> = params
            .tasks
            .iter()
            .filter(|(task_entity, task)| {
                task.is_direct()
                    && params
                        .get_inclusion_modifier
                        .get(current_scenario_entity, *task_entity)
                        .map(|inclusion| **inclusion == Inclusion::Included)
                        .unwrap_or(false)
            })
            .map(|(task_entity, task)| (task_entity, task.clone()))
            .collect();

        if included_direct_tasks.is_empty() {
            ui.label("No included direct tasks in this scenario.");
        } else {
            Grid::new("simulation_tasks").num_columns(1).show(ui, |ui| {
                for (task_entity, task) in &included_direct_tasks {
                    ui.label(format!(
                        "Task {}: {}/{} - {}",
                        task_entity.index(),
                        task.fleet(),
                        task.robot(),
                        task.request().category(),
                    ));
                    ui.end_row();
                }
            });
        }

        if ui.button("Close").clicked() {
            params.display.show = false;
        }
    }
}
