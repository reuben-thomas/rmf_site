//! ```text
//! ┌───────────────────┐     ┌───────┐     ┌─────────┐
//! │ request_generator │     │ robot │     │ planner │
//! └─────────┬─────────┘     └───┬───┘     └────┬────┘
//!           │                   │              │
//!           │ TaskState         │              │
//!           ├──────────────────►│              │
//!           │                   │              │
//!           │                   │ Pose         │
//!           │                   ├─────────────►│
//!           │                   │              │
//!           │                   │              │ AssignTrajectories
//!           │                   │              ├──┐
//!           │                   │              │  │
//!           │                   │              │◄─┘
//!           │                   │              │
//!           │                   │ RobotTrajectory
//!           │                   │◄─────────────┤
//!           │                   │              │
//!
//! ```

use bevy::{
    ecs::query::QueryData,
    ecs::system::{Command, SystemParam, SystemState},
    prelude::*,
};
use rmf_site_editor::SiteEditor;
use rmf_site_editor::color_picker::ColorPicker;
use rmf_site_editor::layers::ZLayer;
use rmf_site_editor::occupancy::{Cell, Grid};
use rmf_site_editor::site::{
    Affiliation, Angle, CircleCollision, CurrentLevel, DifferentialDrive, DoorMarker, GoToPlace,
    LocationTags, NameInSite, Point, Pose, Robot, Rotation, SiteAssets, Task, TaskParams,
    line_stroke_transform,
};
use rmf_site_sim::event::{CandidateComponentEventWriter, CandidateEventWriter};
use rmf_site_sim::playback::SimulationPlaybackPlugin;
use rmf_site_sim::time::SimulationClock;
use rmf_site_sim::time::SimulationTime;
use rmf_site_sim::{SimulationBuilder, SimulationPlugin};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use simulation::*;
use ui::SimulationUiPlugin;
use visualization::*;

const NEGOTIATION_QUEUE_LENGTH_LIMIT: usize = 1_000_000;
const DEFAULT_CELL_SIZE: f32 = 0.5;
const DEFAULT_ROBOT_RADIUS: f32 = 0.2;

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
    use rmf_site_editor::site::{CurrentScenario, GetModifier, Inclusion, Modifier};
    use rmf_site_egui::{
        MenuEvent, MenuItem, PanelConfig, PanelSettings, PanelWidget, PanelWidgetInput,
        ScrollConfig, Tile, ToolMenu, Widget, WidgetSystem, show_panel_of_tiles,
    };
    use rmf_site_sim::interaction::rmf_site_egui::{
        SimulationOverviewTile, SimulationPlaybackTile, show_collapsible_section,
    };

    /// The plugin providing ui functionality including the simulation panel, its tiles, and a menu item to toggle the panel's visibility.
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
        get_inclusion_modifier: GetModifier<'w, 's, Modifier<Inclusion>>,
        tasks: Query<'w, 's, (Entity, &'static Task)>,
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
    }

    impl<'w, 's> WidgetSystem<Tile> for SimulationComputeTile<'w, 's> {
        fn show(_: Tile, ui: &mut Ui, state: &mut SystemState<Self>, world: &mut World) {
            let mut params = state.get_mut(world);
            let tasks = params.direct_included_tasks();

            show_collapsible_section(ui, "Compute", |ui| {
                ui.label(format!("Direct Tasks: {}", tasks.len()));
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut *params.new_simulation_name);

                    let has_any_task = !tasks.is_empty();
                    let has_name = !params.new_simulation_name.trim().is_empty();

                    ui.add_enabled_ui(has_any_task && has_name, |ui| {
                        if ui.button("Compute").clicked() {
                            crate::simulation::spawn_simulation(
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
                            reasons.push("Add at least one direct task included in this scenario.");
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
}

mod simulation {
    use crate::mapf::LinearTrajectorySE2;
    use crate::*;

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
    }

    /// A convenience command for assigning trajectories to all robots in a single [`rmf_site_sim::event::DiscreteEvent`].
    #[derive(Clone, Debug)]
    pub struct AssignRobotTrajectory {
        pub trajectories: Vec<(Entity, RobotTrajectory)>,
    }

    impl Command for AssignRobotTrajectory {
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

    pub fn spawn_simulation(world: &mut World, tasks: &[Entity], name: String) -> Entity {
        let mut setup_state: SystemState<SimulationSetup> = SystemState::new(world);
        let setup = setup_state.get(world);

        let assignments: Vec<_> = tasks
            .iter()
            .filter_map(|task| Some((*task, setup.create_assignment(*task)?)))
            .collect();
        let simulated = setup.simulated_entities();
        let occupancy = setup.create_occupancy();
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
            .register_resource::<Occupancy>()
            .add_prediction_systems((request_generator, robot, planner).chain())
            .add_visualization_systems((animate_robots, draw_robot_paths).chain())
            .build(world);

        world.spawn((simulation, NameInSite(name))).id()
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
        doors: Query<'w, 's, Entity, With<DoorMarker>>,
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

            entities.extend(self.doors.iter());
            entities
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
                robot_name(assignment.robot, &names)
            );
        }
    }

    /// Predicts robot pose and task completion based on the current trajectory and time.
    pub fn robot(
        tasks: Query<(Entity, &TaskState, &RobotGoalAssignment)>,
        trajectories: Query<&RobotTrajectory>,
        poses_now: Query<&Pose>,
        names: Query<&NameInSite>,
        clock: Res<SimulationClock>,
        mut poses: CandidateComponentEventWriter<Pose>,
        mut states: CandidateComponentEventWriter<TaskState>,
    ) {
        let now = clock.now();

        for (task, state, assignment) in tasks.iter() {
            let Some(trajectory) = active_trajectory(task, state, assignment, &trajectories) else {
                continue;
            };
            let Ok(pose) = poses_now.get(assignment.robot) else {
                continue;
            };

            // An instantaneous update to the robot's position.
            let expected = crate::mapf::pose_at(&trajectory.waypoints, now, pose.trans[2]);
            if *pose != expected {
                poses.predict_now(assignment.robot, expected);
                continue;
            }

            // Predict the task's completion.
            let arrival = crate::mapf::finish_time(&trajectory.waypoints);
            states.predict(arrival, task, TaskState::Complete);
            info!(
                "[robot] Scheduled {} to arrive at {arrival:?}",
                robot_name(assignment.robot, &names)
            );
        }
    }

    /// Generates trajectories for robots.
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
                Some((robot, RobotTrajectory { task, waypoints }))
            })
            .collect();

        info!("[planner] Negotiated {expected} trajectories at {now:?}");
        changes.predict_now(AssignRobotTrajectory { trajectories });
    }

    pub fn robot_name(robot: Entity, names: &Query<&NameInSite>) -> String {
        names
            .get(robot)
            .map(|name| name.0.clone())
            .unwrap_or_else(|_| robot.to_string())
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
}

mod mapf {
    use crate::*;
    use ::mapf::motion::{Duration as MapfDuration, Motion, TimePoint};
    use ::mapf::negotiation::{Agent, Scenario as MapfScenario, negotiate};

    pub use ::mapf::negotiation::LinearTrajectorySE2;

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

    pub fn waypoint_positions(trajectory: &LinearTrajectorySE2) -> Vec<Vec2> {
        trajectory
            .iter()
            .map(|waypoint| {
                Vec2::new(
                    waypoint.position.translation.x as f32,
                    waypoint.position.translation.y as f32,
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
            let base = robot_name(*robot, names);
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

            *pose = crate::mapf::pose_at(&trajectory.waypoints, now, pose.trans[2]);
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
                .unwrap_or(DEFAULT_ROBOT_RADIUS);
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
