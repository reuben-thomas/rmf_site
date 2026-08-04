//!
//! ```text
//!     ┌───────────────────┐              ┌─────────┐                  ┌───────┐                     ┌──────┐
//!     │                   │              │         │                  │       │                     │      │
//!     │ request_generator ├─TaskRequest─►│ planner ├──AddTrajectory──►│ robot ├─────DoorCommand────►│ door │
//!     │                   │              │         │                  │       │                     │      │
//!     └───────────────────┘              └─────────┘                  └───┬───┘                     └───┬──┘
//!                                             ▲                           ▲                             │
//!                                             │                           │                             │
//!                                             └──────RobotPoseUpdate──────┴──────────DoorState──────────┘
//! ```
use bevy::{
    ecs::query::QueryData,
    ecs::system::{SystemParam, SystemState},
    prelude::*,
};
use rmf_site_editor::SiteEditor;
use rmf_site_editor::color_picker::ColorPicker;
use rmf_site_editor::layers::ZLayer;
use rmf_site_editor::occupancy::{Cell, Grid};
use rmf_site_editor::site::{
    Affiliation, Angle, CircleCollision, CurrentLevel, DifferentialDrive, DoorMarker, DoorType,
    Edge, GoToPlace, LocationTags, NameInSite, Point, Pose, Robot, Rotation, SiteAssets, Task,
    TaskParams, find_door_position_tfs, line_stroke_transform,
};
use rmf_site_sim::event::{CandidateComponentEventWriter, CandidateEventWriter, DiscreteEvent};
use rmf_site_sim::playback::SimulationPlaybackPlugin;
use rmf_site_sim::time::SimulationClock;
use rmf_site_sim::time::SimulationTime;
use rmf_site_sim::{SimulationBuilder, SimulationPlugin};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use simulation::*;
use ui::SimulationUiPlugin;
use visualization::*;

fn main() {
    App::new()
        .add_plugins((SiteEditor::default(), DiscreteEventSimulationPlugin))
        .run();
}

#[derive(Default)]
struct DiscreteEventSimulationPlugin;

impl Plugin for DiscreteEventSimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            SimulationPlugin,
            SimulationPlaybackPlugin,
            SimulationUiPlugin,
        ));
    }
}

mod ui {
    use crate::*;
    use rmf_site_editor::AppState;
    use rmf_site_editor::bevy_egui::egui::{self, Ui};
    use rmf_site_editor::occupancy::{OccupancyExporter, OccupancyInfo};
    use rmf_site_editor::site::{CurrentScenario, GetModifier, Inclusion, Modifier};
    use rmf_site_egui::{
        MenuEvent, MenuItem, PanelConfig, PanelSettings, PanelWidget, PanelWidgetInput,
        ScrollConfig, Tile, ToolMenu, Widget, WidgetSystem, show_panel_of_tiles,
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
}

mod simulation {
    use crate::mapf::LinearTrajectorySE2;
    use crate::*;

    /// The duration taken by a door to move between fully closed and fully opened states.
    const DOOR_TRANSITION_DURATION: Duration = Duration::from_secs(2);
    /// The distance a robot keeps clear of an obstacle that blocks its trajectory.
    const OBSTACLE_SAFETY_DISTANCE: f32 = 1.0;

    /// A marker component for entities extracted into the simulation world.
    #[derive(Component, Clone, Copy)]
    pub struct SimulationMarker;

    /// The state of a task.
    #[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TaskState {
        Pending,
        Active,
        Complete,
    }

    /// The representation of a task in the simulation world.
    #[derive(Component, Clone, Copy, Debug)]
    pub struct RobotGoalAssignment {
        pub robot: Entity,
        pub goal: Vec2,
    }

    /// A trajectory for a robot, in simulation time.
    #[derive(Component, Clone, Debug)]
    pub struct RobotTrajectory {
        pub task: Entity,
        pub waypoints: LinearTrajectorySE2,
        /// The trajectory time at which the robot is stopped, waiting for an
        /// obstacle to clear, if it is waiting at all.
        pub held_at: Option<SimulationTime>,
    }

    impl RobotTrajectory {
        pub fn new(task: Entity, waypoints: LinearTrajectorySE2) -> Self {
            Self {
                task,
                waypoints,
                held_at: None,
            }
        }

        /// The trajectory time that the robot has reached at `now`.
        pub fn time(&self, now: SimulationTime) -> SimulationTime {
            self.held_at.unwrap_or(now)
        }

        /// The same trajectory, with the robot stopped at trajectory time `at`.
        pub fn held(&self, at: SimulationTime) -> Self {
            Self {
                held_at: Some(at),
                ..self.clone()
            }
        }

        /// The same trajectory, postponed by however long the robot has been
        /// waiting so that it continues from where it stopped.
        pub fn resumed(&self, now: SimulationTime) -> Self {
            let waited = now.elapsed() - self.time(now).elapsed();
            Self {
                task: self.task,
                waypoints: crate::mapf::postponed(&self.waypoints, waited),
                held_at: None,
            }
        }
    }

    /// A convenience command for assigning trajectories to all robots in a single [`rmf_site_sim::event::DiscreteEvent`].
    #[derive(Clone, Debug)]
    pub struct AssignRobotTrajectory {
        pub trajectories: Vec<(Entity, RobotTrajectory)>,
    }

    impl DiscreteEvent for AssignRobotTrajectory {
        fn apply(self, world: &mut World) {
            for (robot, trajectory) in self.trajectories {
                world.entity_mut(robot).insert(trajectory);
            }
        }
    }

    /// An occupancy grid for the current level accounting for all non-simulation entities.
    #[derive(Resource, Clone, Debug)]
    pub struct Occupancy {
        pub occupied: HashSet<Cell>,
        pub cell_size: f32,
    }

    /// The state that a door is requested to move into.
    #[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
    pub enum DoorCommand {
        Open,
        Close,
    }

    /// The state of a door.
    #[derive(Component, Clone, Copy, Debug, PartialEq)]
    pub enum DoorState {
        Closed,
        Opening { since: SimulationTime },
        Open,
        Closing { since: SimulationTime },
    }

    impl DoorState {
        pub fn is_open(&self) -> bool {
            matches!(self, DoorState::Open)
        }

        /// How far the door has moved from fully closed (0.0) to fully open (1.0) at `now`.
        pub fn position(&self, now: SimulationTime) -> f32 {
            let progress = |since: SimulationTime| {
                let elapsed = now.elapsed().saturating_sub(since.elapsed());
                (elapsed.as_secs_f32() / DOOR_TRANSITION_DURATION.as_secs_f32()).clamp(0.0, 1.0)
            };

            match self {
                DoorState::Closed => 0.0,
                DoorState::Opening { since } => progress(*since),
                DoorState::Open => 1.0,
                DoorState::Closing { since } => 1.0 - progress(*since),
            }
        }
    }

    /// The cells occupied by a door across its entire range of motion.
    #[derive(Component, Clone, Debug)]
    pub struct DoorOccupancyCells(pub HashSet<Cell>);

    pub fn spawn_simulation(world: &mut World, tasks: &[Entity], name: String) -> Entity {
        let mut setup_state: SystemState<SimulationSetup> = SystemState::new(world);
        let setup = setup_state.get(world);

        let assignments: Vec<_> = tasks
            .iter()
            .filter_map(|task| Some((*task, setup.create_assignment(*task)?)))
            .collect();
        let simulated = setup.simulated_entities();
        let occupancy = setup.create_occupancy();
        let door_cells = setup.create_door_occupancy_cells(occupancy.cell_size);
        world.insert_resource(occupancy);

        for entity in simulated {
            world.entity_mut(entity).insert(SimulationMarker);
        }
        for task in tasks {
            world
                .entity_mut(*task)
                .insert((SimulationMarker, TaskState::Pending));
        }
        for (task, assignment) in assignments {
            world.entity_mut(task).insert(assignment);
        }
        for (door, cells) in door_cells {
            world
                .entity_mut(door)
                .insert((cells, DoorState::Closed, DoorCommand::Close));
        }

        let simulation = SimulationBuilder::<SimulationMarker>::new()
            .register_component::<NameInSite>()
            .register_component::<TaskParams>()
            .register_component::<TaskState>()
            .register_component::<RobotGoalAssignment>()
            .register_component::<Pose>()
            .register_component::<RobotTrajectory>()
            .register_component::<Affiliation<Entity>>()
            .register_component::<DifferentialDrive>()
            .register_component::<CircleCollision>()
            .register_component::<DoorCommand>()
            .register_component::<DoorState>()
            .register_component::<DoorOccupancyCells>()
            .register_component::<DoorType>()
            .register_resource::<Occupancy>()
            .add_prediction_systems((request_generator, robot, door, planner).chain())
            .add_visualization_systems((animate_robots, animate_doors, draw_robot_paths).chain())
            .build(world);

        world.spawn((simulation, NameInSite(name))).id()
    }

    /// Activates tasks by changing their state.
    pub fn request_generator(
        tasks: Query<(
            Entity,
            &TaskState,
            &RobotGoalAssignment,
            Option<&TaskParams>,
        )>,
        names: Query<&NameInSite>,
        mut states: CandidateComponentEventWriter<TaskState>,
    ) {
        for (task, state, assignment, params) in tasks.iter() {
            if *state != TaskState::Pending {
                continue;
            }

            let request_time_millis = params
                .and_then(|params| params.request_time())
                .unwrap_or(0)
                .max(0) as u64;
            let time = SimulationTime::new(Duration::from_millis(request_time_millis));
            states.predict(time, task, TaskState::Active);

            info!(
                "[request_generator] Scheduled the request for {} at {time:?}",
                name_of(assignment.robot, &names)
            );
        }
    }

    /// Predicts robot pose and task completion based on the current trajectory and time.
    pub fn robot(
        tasks: Query<(Entity, &TaskState, &RobotGoalAssignment)>,
        trajectories: Query<&RobotTrajectory>,
        poses_now: Query<&Pose>,
        doors: Query<(Entity, &DoorState, &DoorCommand, &DoorOccupancyCells)>,
        names: Query<&NameInSite>,
        occupancy: Res<Occupancy>,
        clock: Res<SimulationClock>,
        mut states: CandidateComponentEventWriter<TaskState>,
        mut poses: CandidateComponentEventWriter<Pose>,
        mut paths: CandidateComponentEventWriter<RobotTrajectory>,
        mut commands: CandidateComponentEventWriter<DoorCommand>,
    ) {
        let now = clock.now();
        let mut must_open = HashSet::new();

        for (task, state, assignment) in tasks.iter() {
            let Some(trajectory) = active_trajectory(task, state, assignment, &trajectories) else {
                continue;
            };
            let Ok(pose) = poses_now.get(assignment.robot) else {
                continue;
            };
            let reached = trajectory.time(now);

            // An instantaneous update to the robot's position.
            let expected = crate::mapf::pose_at(&trajectory.waypoints, reached, pose.trans[2]);
            if *pose != expected {
                poses.predict_now(assignment.robot, expected);
            }

            // The doors that the robot has yet to clear, and when it comes
            // within and moves back out of [`OBSTACLE_SAFETY_DISTANCE`] of them.
            let ahead: Vec<_> = doors
                .iter()
                .filter_map(|(door, state, _, cells)| {
                    let (approach, clear) = crate::mapf::proximity_interval(
                        &trajectory.waypoints,
                        &cells.0,
                        occupancy.cell_size,
                        OBSTACLE_SAFETY_DISTANCE,
                    )?;
                    (clear > reached).then_some((door, state.is_open(), approach, clear))
                })
                .collect();
            // A robot only asks for a door once it has arrived at a safe
            // distance from it, and may not go any closer until it is open.
            must_open.extend(
                ahead
                    .iter()
                    .filter(|(_, _, approach, _)| *approach <= reached)
                    .map(|(door, ..)| *door),
            );
            let blocked = ahead
                .iter()
                .any(|(_, open, approach, _)| !open && *approach <= reached);

            match (trajectory.held_at.is_some(), blocked) {
                (false, true) => {
                    info!(
                        "[robot] {} is waiting for a door at {now:?}",
                        name_of(assignment.robot, &names)
                    );
                    paths.predict_now(assignment.robot, trajectory.held(reached));
                    continue;
                }
                (true, false) => {
                    info!(
                        "[robot] {} resumed at {now:?}",
                        name_of(assignment.robot, &names)
                    );
                    paths.predict_now(assignment.robot, trajectory.resumed(now));
                    continue;
                }
                // The robot holds its position until a door opens for it.
                (true, true) => continue,
                (false, false) => {}
            }

            // Move the robot to the next door it approaches or clears, and
            // predict the task's completion at the end of its trajectory.
            let arrival = crate::mapf::finish_time(&trajectory.waypoints);
            if let Some(next) = ahead
                .iter()
                .flat_map(|(_, _, approach, clear)| [*approach, *clear])
                .chain([arrival])
                .filter(|time| *time > reached)
                .min()
            {
                let moved = crate::mapf::pose_at(&trajectory.waypoints, next, pose.trans[2]);
                poses.predict(next, assignment.robot, moved);
            }
            states.predict(arrival, task, TaskState::Complete);
        }

        for (door, _, commanded, _) in doors.iter() {
            let command = if must_open.contains(&door) {
                DoorCommand::Open
            } else {
                DoorCommand::Close
            };
            if *commanded != command {
                info!(
                    "[robot] Commanded {} to {command:?} at {now:?}",
                    name_of(door, &names)
                );
                commands.predict_now(door, command);
            }
        }
    }

    /// Moves doors towards the state that their [`DoorCommand`] requests.
    pub fn door(
        doors: Query<(Entity, &DoorState, &DoorCommand)>,
        names: Query<&NameInSite>,
        clock: Res<SimulationClock>,
        mut states: CandidateComponentEventWriter<DoorState>,
    ) {
        let now = clock.now();

        for (door, state, command) in doors.iter() {
            let (time, next) = match (state, command) {
                // A door in motion always comes to rest before it reverses.
                (DoorState::Opening { since }, _) => {
                    (*since + DOOR_TRANSITION_DURATION, DoorState::Open)
                }
                (DoorState::Closing { since }, _) => {
                    (*since + DOOR_TRANSITION_DURATION, DoorState::Closed)
                }
                (DoorState::Closed, DoorCommand::Open) => (now, DoorState::Opening { since: now }),
                (DoorState::Open, DoorCommand::Close) => (now, DoorState::Closing { since: now }),
                _ => continue,
            };

            states.predict(time, door, next);
            info!(
                "[door] Scheduled {} to be {next:?} at {time:?}",
                name_of(door, &names)
            );
        }
    }

    /// Generates trajectories for robots.
    ///
    /// Doors are not obstacles, they are opened on demand by [`robot`] for the
    /// trajectories planned here.
    pub fn planner(
        tasks: Query<(Entity, &TaskState, &RobotGoalAssignment)>,
        robots: Query<(&Pose, &Affiliation<Entity>, Option<&RobotTrajectory>)>,
        descriptions: Query<(&DifferentialDrive, &CircleCollision)>,
        names: Query<&NameInSite>,
        occupancy: Res<Occupancy>,
        clock: Res<SimulationClock>,
        mut changes: CandidateEventWriter,
    ) {
        let mut agents = HashMap::new();
        let mut assigned_tasks = HashMap::new();
        let mut trajectory_required = false;

        for (task, state, assignment) in tasks.iter() {
            if *state != TaskState::Active {
                continue;
            }

            let (pose, affiliation, trajectory) = robots
                .get(assignment.robot)
                .expect("An active robot has no pose, affiliation, or trajectory.");
            let (drive, collision) = affiliation
                .0
                .and_then(|description| descriptions.get(description).ok())
                .expect("An agent has insufficient collision or drive information for trajectory planning.");
            trajectory_required |= trajectory.is_none_or(|trajectory| trajectory.task != task);
            agents.insert(
                assignment.robot,
                crate::mapf::agent(pose, assignment.goal, drive, collision, occupancy.cell_size),
            );
            assigned_tasks.insert(assignment.robot, task);
        }

        if !trajectory_required || agents.is_empty() {
            return;
        }

        let now = clock.now();
        let expected = agents.len();
        let Some(negotiated) = crate::mapf::negotiate_trajectories(agents, &names, &occupancy, now)
        else {
            return;
        };
        let trajectories: Vec<_> = negotiated
            .into_iter()
            .filter_map(|(robot, waypoints)| {
                let task = *assigned_tasks.get(&robot)?;
                Some((robot, RobotTrajectory::new(task, waypoints)))
            })
            .collect();

        info!("[planner] Negotiated {expected} trajectories at {now:?}");
        changes.predict_now(AssignRobotTrajectory { trajectories });
    }

    pub fn name_of(entity: Entity, names: &Query<&NameInSite>) -> String {
        names
            .get(entity)
            .map(|name| name.0.clone())
            .unwrap_or_else(|_| entity.to_string())
    }

    pub fn active_trajectory<'a>(
        task: Entity,
        state: &TaskState,
        assignment: &RobotGoalAssignment,
        trajectories: &'a Query<&RobotTrajectory>,
    ) -> Option<&'a RobotTrajectory> {
        if *state != TaskState::Active {
            return None;
        }
        let trajectory = trajectories.get(assignment.robot).ok()?;
        (trajectory.task == task).then_some(trajectory)
    }

    /// The cells that a door occupies across every position of its motion.
    ///
    /// A swinging door sweeps out a sector of the floor rather than only the
    /// line between its anchors, so a robot must be clear of the whole sector
    /// before the door may move.
    pub fn swept_cells(left: Vec2, right: Vec2, kind: &DoorType, cell_size: f32) -> HashSet<Cell> {
        // A sample every half cell along the widest arc a panel can sweep.
        let steps = ((left.distance(right) * std::f32::consts::PI) / (cell_size / 2.0)).ceil();
        let steps = (steps as usize).max(1);

        let mut cells = HashSet::new();
        let mut swept = kind.clone();
        for step in 0..=steps {
            swept.set_positions(step as f32 / steps as f32);
            for (start, end) in door_panels(left, right, &swept) {
                cells.extend(get_cells_along(&[start, end], cell_size));
            }
        }
        cells
    }

    /// The endpoints of each of a door's panels, in the same frame as its anchors.
    fn door_panels(left: Vec2, right: Vec2, kind: &DoorType) -> Vec<(Vec2, Vec2)> {
        let length = left.distance(right);
        let offset = match kind {
            DoorType::DoubleSliding(door) => door.compute_offset(length),
            DoorType::DoubleSwing(door) => door.compute_offset(length),
            _ => 0.0,
        };

        // The door frame is centered between the anchors with its y axis facing
        // the left anchor.
        let origin = (left + right) / 2.0;
        let y_axis = (left - right).normalize_or_zero();
        let x_axis = Vec2::new(y_axis.y, -y_axis.x);
        let in_site = |point: Vec2| origin + x_axis * point.x + y_axis * point.y;

        find_door_position_tfs(kind, length, offset)
            .into_iter()
            .map(|panel| {
                let center = panel.translation.truncate();
                let half_span = (panel.rotation * Vec3::Y).truncate() * panel.scale.y / 2.0;
                (in_site(center - half_span), in_site(center + half_span))
            })
            .collect()
    }

    pub fn get_cells_along(points: &[Vec2], cell_size: f32) -> HashSet<Cell> {
        let mut cells: HashSet<Cell> = points
            .iter()
            .map(|point| Cell::from_point(*point, cell_size))
            .collect();

        for pair in points.windows(2) {
            let steps = (pair[0].distance(pair[1]) / (cell_size / 2.0)).ceil() as usize;
            for step in 1..steps {
                let point = pair[0].lerp(pair[1], step as f32 / steps as f32);
                cells.insert(Cell::from_point(point, cell_size));
            }
        }
        cells
    }

    /// A helper [`SystemParam`] for setting up the simulation.
    #[derive(SystemParam)]
    pub struct SimulationSetup<'w, 's> {
        tasks: Query<'w, 's, (&'static Task, &'static GoToPlace)>,
        robots: Query<
            'w,
            's,
            (
                Entity,
                &'static NameInSite,
                Option<&'static Affiliation<Entity>>,
            ),
            With<Robot>,
        >,
        doors: Query<'w, 's, (Entity, &'static Edge<Entity>, &'static DoorType), With<DoorMarker>>,
        locations: Query<'w, 's, (&'static NameInSite, &'static Point<Entity>), With<LocationTags>>,
        anchors: Query<'w, 's, &'static GlobalTransform>,
        grids: Query<'w, 's, (&'static Grid, &'static ChildOf)>,
        current_level: Res<'w, CurrentLevel>,
    }

    impl SimulationSetup<'_, '_> {
        fn simulated_entities(&self) -> Vec<Entity> {
            let mut entities: Vec<Entity> = self.robots.iter().map(|(robot, ..)| robot).collect();

            entities.extend(
                self.robots
                    .iter()
                    .filter_map(|(_, _, affiliation)| affiliation.and_then(|a| a.0)),
            );

            entities.extend(self.doors.iter().map(|(door, ..)| door));
            entities
        }

        fn create_door_occupancy_cells(&self, cell_size: f32) -> Vec<(Entity, DoorOccupancyCells)> {
            self.doors
                .iter()
                .filter_map(|(door, edge, kind)| {
                    let left = self.anchors.get(edge.left()).ok()?.translation().truncate();
                    let right = self
                        .anchors
                        .get(edge.right())
                        .ok()?
                        .translation()
                        .truncate();
                    Some((
                        door,
                        DoorOccupancyCells(swept_cells(left, right, kind, cell_size)),
                    ))
                })
                .collect()
        }

        fn create_assignment(&self, task: Entity) -> Option<RobotGoalAssignment> {
            let (request, go_to_place) = self.tasks.get(task).ok()?;
            let (robot, _, _) = self
                .robots
                .iter()
                .find(|(_, name, _)| name.0 == request.robot())?;
            let (_, Point(anchor)) = self
                .locations
                .iter()
                .find(|(name, _)| name.0 == go_to_place.location)?;
            let goal = self.anchors.get(*anchor).ok()?.translation().truncate();

            Some(RobotGoalAssignment { robot, goal })
        }

        fn create_occupancy(&self) -> Occupancy {
            let level = self.current_level.0.expect("No current level set");
            let (grid, _) = self
                .grids
                .iter()
                .find(|(_, child_of)| child_of.parent() == level)
                .expect("No occupancy grid found for the current level");

            Occupancy {
                occupied: grid.occupied.clone(),
                cell_size: grid.cell_size,
            }
        }
    }
}

mod mapf {
    use crate::*;
    use ::mapf::motion::{Duration as MapfDuration, Motion, TimePoint};
    use ::mapf::negotiation::{Agent, Scenario as MapfScenario, negotiate};

    pub use ::mapf::negotiation::LinearTrajectorySE2;

    /// The maximum size that the search queue size can reach in mapf.
    const NEGOTIATION_QUEUE_LENGTH_LIMIT: usize = 1_000_000;

    pub fn agent(
        pose: &Pose,
        goal: Vec2,
        drive: &DifferentialDrive,
        collision: &CircleCollision,
        cell_size: f32,
    ) -> Agent {
        let cell = |point: Vec2| Cell::from_point(point, cell_size).to_xy();
        Agent {
            start: cell(Vec2::new(pose.trans[0], pose.trans[1])),
            yaw: f64::from(pose.rot.yaw().radians()),
            goal: cell(goal),
            radius: f64::from(collision.radius),
            speed: f64::from(drive.translational_speed),
            spin: f64::from(drive.rotational_speed),
        }
    }

    pub fn negotiate_trajectories(
        agents: HashMap<Entity, Agent>,
        names: &Query<&NameInSite>,
        occupancy: &Occupancy,
        start_time: SimulationTime,
    ) -> Option<Vec<(Entity, LinearTrajectorySE2)>> {
        let robots = agent_names(&agents, names);
        let scenario = MapfScenario {
            agents: robots
                .iter()
                .map(|(name, robot)| (name.clone(), agents[robot]))
                .collect(),
            obstacles: Vec::new(),
            occupancy: occupancy_map(occupancy),
            cell_size: f64::from(occupancy.cell_size),
            camera_bounds: None,
        };

        let (solution, _, name_map) = negotiate(&scenario, Some(NEGOTIATION_QUEUE_LENGTH_LIMIT))
            .inspect_err(|err| error!("[planner] Negotiation failed: {err}"))
            .ok()?;

        Some(
            solution
                .proposals
                .iter()
                .filter_map(|(id, proposal)| {
                    let robot = name_map
                        .get(id)
                        .and_then(|name| robots.get(name))
                        .copied()?;
                    let mut trajectory = proposal.meta.trajectory.clone();
                    trajectory.adjust_times(MapfDuration::from_std_duration(start_time.elapsed()));
                    // Holding the endpoints keeps the trajectory defined for all time.
                    Some((
                        robot,
                        trajectory
                            .with_indefinite_initial_time(true)
                            .with_indefinite_finish_time(true),
                    ))
                })
                .collect(),
        )
    }

    pub fn pose_at(trajectory: &LinearTrajectorySE2, time: SimulationTime, z: f32) -> Pose {
        let position = trajectory
            .motion()
            .compute_position(&time_point(time))
            .unwrap_or_else(|_| trajectory.initial_motion().position);

        Pose {
            trans: [
                position.translation.x as f32,
                position.translation.y as f32,
                z,
            ],
            rot: Rotation::Yaw(Angle::Rad(position.rotation.angle() as f32)),
        }
    }

    pub fn finish_time(trajectory: &LinearTrajectorySE2) -> SimulationTime {
        SimulationTime::new(
            trajectory
                .finish_motion_time()
                .duration_from_zero()
                .into_std_duration(),
        )
    }

    /// The same trajectory, delayed by `postponement`.
    pub fn postponed(
        trajectory: &LinearTrajectorySE2,
        postponement: Duration,
    ) -> LinearTrajectorySE2 {
        let mut postponed = trajectory.clone();
        postponed.adjust_times(MapfDuration::from_std_duration(postponement));
        postponed
    }

    /// The interval over which a trajectory is within `distance` of `cells`,
    /// from when it approaches them to when it has cleared them.
    ///
    /// A trajectory that approaches `cells` more than once yields a single
    /// interval spanning every visit.
    pub fn proximity_interval(
        trajectory: &LinearTrajectorySE2,
        cells: &HashSet<Cell>,
        cell_size: f32,
        distance: f32,
    ) -> Option<(SimulationTime, SimulationTime)> {
        let samples = sample_motion(trajectory, cell_size);
        let near = |index: &usize| {
            let point = samples[*index].0;
            cells
                .iter()
                .any(|cell| cell.to_center_point(cell_size).distance(point) <= distance)
        };

        // The interval is bounded by the samples on either side of the cells,
        // at which the trajectory is still `distance` clear of them.
        let approach = samples[(0..samples.len()).find(near)?.saturating_sub(1)].1;
        let clear = samples[((0..samples.len()).rfind(near)? + 1).min(samples.len() - 1)].1;
        Some((approach, clear))
    }

    pub fn waypoint_positions(trajectory: &LinearTrajectorySE2) -> Vec<Vec2> {
        waypoints(trajectory)
            .into_iter()
            .map(|(position, _)| position)
            .collect()
    }

    /// The position of a trajectory sampled at sub-cell intervals, and the time at each sample.
    fn sample_motion(
        trajectory: &LinearTrajectorySE2,
        cell_size: f32,
    ) -> Vec<(Vec2, SimulationTime)> {
        let waypoints = waypoints(trajectory);
        let mut samples = Vec::new();

        for pair in waypoints.windows(2) {
            let ((from, departure), (to, arrival)) = (pair[0], pair[1]);
            let steps = (from.distance(to) / (cell_size / 2.0)).ceil().max(1.0) as usize;
            for step in 0..steps {
                let progress = step as f32 / steps as f32;
                let elapsed = (arrival.elapsed() - departure.elapsed()).mul_f32(progress);
                samples.push((from.lerp(to, progress), departure + elapsed));
            }
        }
        samples.extend(waypoints.last().copied());
        samples
    }

    /// The position and time of each waypoint of a trajectory.
    fn waypoints(trajectory: &LinearTrajectorySE2) -> Vec<(Vec2, SimulationTime)> {
        trajectory
            .iter()
            .map(|waypoint| {
                (
                    Vec2::new(
                        waypoint.position.translation.x as f32,
                        waypoint.position.translation.y as f32,
                    ),
                    SimulationTime::new(waypoint.time.duration_from_zero().into_std_duration()),
                )
            })
            .collect()
    }

    fn occupancy_map(occupancy: &Occupancy) -> HashMap<i64, Vec<i64>> {
        let mut map = HashMap::<i64, Vec<i64>>::new();
        for cell in occupancy.occupied.iter() {
            map.entry(cell.y).or_default().push(cell.x);
        }
        for column in map.values_mut() {
            column.sort_unstable();
        }
        map
    }

    fn agent_names(
        agents: &HashMap<Entity, Agent>,
        names: &Query<&NameInSite>,
    ) -> HashMap<String, Entity> {
        let mut robots = HashMap::new();
        for robot in agents.keys() {
            let base = name_of(*robot, names);
            let mut name = base.clone();
            for suffix in 2.. {
                if !robots.contains_key(&name) {
                    break;
                }
                name = format!("{base} ({suffix})");
            }
            robots.insert(name, *robot);
        }
        robots
    }

    /// The mapf time point equivalent to a simulation time.
    fn time_point(time: SimulationTime) -> TimePoint {
        TimePoint::zero() + MapfDuration::from_std_duration(time.elapsed())
    }
}

mod visualization {
    use crate::*;

    /// A marker component for a mesh drawing part of a robot's trajectory.
    #[derive(Component)]
    pub struct SimulationPathVisual;

    pub fn animate_robots(
        tasks: Query<(Entity, &TaskState, &RobotGoalAssignment)>,
        trajectories: Query<&RobotTrajectory>,
        mut poses: Query<&mut Pose>,
        clock: Res<SimulationClock>,
    ) {
        let now = clock.now();

        for (task, state, assignment) in tasks.iter() {
            let Some(trajectory) = active_trajectory(task, state, assignment, &trajectories) else {
                continue;
            };
            let Ok(mut pose) = poses.get_mut(assignment.robot) else {
                continue;
            };

            *pose =
                crate::mapf::pose_at(&trajectory.waypoints, trajectory.time(now), pose.trans[2]);
        }
    }

    pub fn animate_doors(
        doors: Query<(Entity, &DoorState)>,
        mut kinds: Query<&mut DoorType>,
        clock: Res<SimulationClock>,
    ) {
        let now = clock.now();

        for (door, state) in doors.iter() {
            let Ok(mut kind) = kinds.get_mut(door) else {
                continue;
            };

            let mut moved = kind.clone();
            moved.set_positions(state.position(now));
            kind.set_if_neq(moved);
        }
    }

    pub fn draw_robot_paths(
        tasks: Query<(Entity, &TaskState, &RobotGoalAssignment)>,
        trajectories: Query<&RobotTrajectory>,
        collisions: Query<&CircleCollision>,
        affiliations: Query<&Affiliation<Entity>>,
        site_assets: Res<SiteAssets>,
        current_level: Res<CurrentLevel>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        mut visuals: Query<PathVisualMeshes, With<SimulationPathVisual>>,
        mut pool: Local<Vec<Entity>>,
        mut robot_materials: Local<HashMap<Entity, Handle<StandardMaterial>>>,
        mut commands: Commands,
    ) {
        let Some(level) = current_level.0 else {
            return;
        };

        let mut meshes = Vec::new();
        for (task, state, assignment) in tasks.iter() {
            let Some(trajectory) = active_trajectory(task, state, assignment, &trajectories) else {
                continue;
            };

            let radius = affiliations
                .get(assignment.robot)
                .ok()
                .and_then(|affiliation| affiliation.0)
                .and_then(|description| collisions.get(description).ok())
                .map(|collision| collision.radius)
                .expect("Robots should have a collision radius");
            let material = robot_materials
                .entry(assignment.robot)
                .or_insert_with(|| materials.add(path_material()))
                .clone();

            let positions = crate::mapf::waypoint_positions(&trajectory.waypoints);
            for pair in positions.windows(2) {
                let start = pair[0].extend(ZLayer::RobotPath.to_z());
                let end = pair[1].extend(ZLayer::RobotPath.to_z());
                let scale = Vec3::new(radius, radius, 1.0);
                meshes.extend([
                    (
                        site_assets.robot_path_rectangle_mesh.clone(),
                        line_stroke_transform(&start, &end, radius * 2.0),
                        material.clone(),
                    ),
                    (
                        site_assets.robot_path_circle_mesh.clone(),
                        Transform::from_translation(start).with_scale(scale),
                        material.clone(),
                    ),
                    (
                        site_assets.robot_path_circle_mesh.clone(),
                        Transform::from_translation(end).with_scale(scale),
                        material.clone(),
                    ),
                ]);
            }
        }

        let drawn = meshes.len();
        for (index, (mesh, transform, material)) in meshes.into_iter().enumerate() {
            let Some(mut visual) = pool.get(index).and_then(|e| visuals.get_mut(*e).ok()) else {
                let entity = commands
                    .spawn((
                        SimulationPathVisual,
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        transform,
                        ChildOf(level),
                    ))
                    .id();
                match pool.get_mut(index) {
                    Some(pooled) => *pooled = entity,
                    None => pool.push(entity),
                }
                continue;
            };

            *visual.transform = transform;
            *visual.visibility = Visibility::Inherited;
            visual.mesh.0 = mesh;
            visual.material.0 = material;
        }

        for entity in pool.iter().skip(drawn) {
            if let Ok(mut visual) = visuals.get_mut(*entity) {
                *visual.visibility = Visibility::Hidden;
            }
        }
    }

    #[derive(QueryData)]
    #[query_data(mutable)]
    pub struct PathVisualMeshes {
        transform: &'static mut Transform,
        visibility: &'static mut Visibility,
        mesh: &'static mut Mesh3d,
        material: &'static mut MeshMaterial3d<StandardMaterial>,
    }

    fn path_material() -> StandardMaterial {
        StandardMaterial {
            base_color: Color::srgb_from_array(ColorPicker::get_color()),
            unlit: true,
            ..Default::default()
        }
    }
}
