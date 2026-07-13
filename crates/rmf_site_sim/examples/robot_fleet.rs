//! In this example, we set up, compute, and animate a simple discrete event simulation for a robot fleet.
//! A planner reacts to requests, scheduling trajectories for robots to reach a goal waypoint.
//! A robot controller schedules transform updates along the trajectory, which a robot tracker observes as they are applied.
//! Upon reaching a goal waypoint, the waypoint is marked as visited.
//!
//!
//! ```
//! --[Request..]--> Planner --[Trajectory..]--> RobotController --[Transform updates]--> RobotTracker
//!                                                              \--[GoalReached]--> Waypoint
//! ```

use bevy::color::palettes::basic;
use bevy::prelude::*;
use rand::Rng;
use rmf_site_sim::compute::SimulationComputeClock;
use rmf_site_sim::event::DiscreteChange;
use rmf_site_sim::time::SimulationTime;
use rmf_site_sim::{Simulation, SimulationBuilder, SimulationPlugin};
use std::time::Duration;

/// A marker component for a robot.
#[derive(Component, Clone, Debug)]
struct Robot;

/// A waypoint, which can be visited or unvisited.
#[derive(Component, Clone, Debug, Eq, PartialEq)]
enum Waypoint {
    Visited,
    Unvisited,
}

#[derive(Component, Clone)]
struct Request {
    goal: Entity,
}

#[derive(Component, Clone)]
struct Trajectory {
    points: Vec<TrajectoryPoint>,
}

#[derive(Clone)]
struct TrajectoryPoint {
    time: SimulationTime,
    transform: Transform,
}

#[derive(Component, Clone)]
struct GoalReached {
    robot: Entity,
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, SimulationPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, (simple_playback, draw_robots, draw_waypoints))
        .run();
}

fn setup(world: &mut World) {
    let position_bounds = Rect::from_center_half_size(Vec2::ZERO, Vec2::new(250.0, 250.0));

    world.spawn((
        Camera2d,
        Transform::from_translation(position_bounds.center().extend(0.0)),
    ));

    spawn_robots_waypoints(world, 5, position_bounds);

    let simulation = SimulationBuilder::new()
        // Untracked components may not change, or may not be useful for visualization.
        .register_untracked_component::<Robot>()
        .register_untracked_component::<Name>()
        // Changes to positions and waypoint states are relevant to the simulation and should be tracked.
        .register_tracked_component::<Transform>()
        .register_tracked_component::<Waypoint>()
        // Systems representing models used to compute the simulation.
        .add_startup_systems(schedule_initial_requests)
        .add_compute_systems((planner, robot_controller, robot_tracker, waypoint))
        .build(world);

    world.spawn(simulation);
}

fn spawn_robots_waypoints(world: &mut World, n: usize, position_bounds: Rect) {
    let mut rng = rand::thread_rng();

    for i in 0..n {
        world.spawn((
            Robot,
            Name::new(format!("Robot{i}")),
            random_transform(&mut rng, position_bounds),
        ));
        world.spawn((
            Waypoint::Unvisited,
            Name::new(format!("Waypoint{i}")),
            random_transform(&mut rng, position_bounds),
        ));
    }
}

/// Schedules initial requests, assigning robots to unvisited waypoints arbitrarily.
fn schedule_initial_requests(
    mut changes: DiscreteChange,
    robots: Query<(Entity, &Name), With<Robot>>,
    waypoints: Query<(Entity, &Name, &Waypoint)>,
) {
    let mut schedule_time_secs = 1;
    let mut unvisited = waypoints
        .iter()
        .filter(|(_, _, waypoint)| **waypoint == Waypoint::Unvisited);

    for (robot_entity, robot_name) in robots.iter() {
        let Some((waypoint_entity, waypoint_name, _)) = unvisited.next() else {
            break;
        };

        changes.schedule(
            SimulationTime::new(Duration::from_secs(schedule_time_secs)),
            move |world: &mut World| {
                world.entity_mut(robot_entity).insert(Request {
                    goal: waypoint_entity,
                });
            },
        );
        info!("Scheduled request for {} to {}", robot_name, waypoint_name);
        schedule_time_secs += 1;
    }
}

/// A planner that plans robot trajectories using linear interpolation.
fn planner(
    requests: Query<(Entity, &Transform, &Name, &Request), Added<Request>>,
    goals: Query<&Transform, With<Waypoint>>,
    mut changes: DiscreteChange,
    clock: Res<SimulationComputeClock>,
) {
    let mut planned_time = clock.now();

    for (robot, current_transform, robot_name, request) in requests.iter() {
        // Simulate that each request was processed in sequence with 2 seconds of latency for processing
        planned_time = SimulationTime::new(planned_time.elapsed() + Duration::from_secs(2));
        let target_transform = goals.get(request.goal).unwrap();

        // Linearly interpolate between the current and target transforms
        let travel_duration = Duration::from_secs(10);
        let interpolation_steps = 10;
        let points = (0..=interpolation_steps)
            .map(|current_step| {
                let progress = current_step as f32 / interpolation_steps as f32;
                TrajectoryPoint {
                    time: SimulationTime::new(
                        planned_time.elapsed() + travel_duration.mul_f32(progress),
                    ),
                    transform: Transform {
                        translation: current_transform
                            .translation
                            .lerp(target_transform.translation, progress),
                        rotation: current_transform
                            .rotation
                            .slerp(target_transform.rotation, progress),
                        scale: current_transform.scale,
                    },
                }
            })
            .collect();

        let trajectory = Trajectory { points };
        changes.schedule(planned_time, move |world: &mut World| {
            world.entity_mut(robot).insert(trajectory);
        });
        info!(
            "Planned trajectory for {} at {:?}",
            robot_name, planned_time
        );
    }
}

/// A robot controller that schedules transform updates along the trajectory,
/// and notifies when goal waypoints are reached.
fn robot_controller(
    trajectories: Query<(Entity, &Trajectory), Added<Trajectory>>,
    waypoints: Query<(Entity, &Transform), (With<Waypoint>, Without<Robot>)>,
    mut changes: DiscreteChange,
) {
    for (robot, trajectory) in trajectories.iter() {
        info!("Scheduling trajectory for robot {:?}", robot);
        for point in trajectory.points.iter().skip(1) {
            let transform = point.transform;
            changes.schedule(point.time, move |world: &mut World| {
                *world.get_mut::<Transform>(robot).unwrap() = transform;
            });
        }

        let trajectory_end = trajectory.points.last().unwrap();
        let reached = waypoints.iter().find(|(_, waypoint_transform)| {
            waypoint_transform
                .translation
                .distance(trajectory_end.transform.translation)
                == 0.0
        });
        if let Some((waypoint, _)) = reached {
            changes.schedule(trajectory_end.time, move |world: &mut World| {
                world.entity_mut(waypoint).insert(GoalReached { robot });
            });
        }
    }
}

/// A robot tracker that observes robot position updates as they are applied.
fn robot_tracker(
    robots: Query<&Name, (With<Robot>, Changed<Transform>)>,
    clock: Res<SimulationComputeClock>,
) {
    for robot_name in robots.iter() {
        info!("Updated state for {} at {:?}", robot_name, clock.now());
    }
}

/// A waypoint that is marked as visited when a robot reaches it.
fn waypoint(
    mut waypoints: Query<(&Name, &mut Waypoint, &GoalReached), Added<GoalReached>>,
    robots: Query<&Name, With<Robot>>,
    clock: Res<SimulationComputeClock>,
) {
    for (waypoint_name, mut waypoint, goal_reached) in waypoints.iter_mut() {
        let robot_name = robots.get(goal_reached.robot).unwrap();
        *waypoint = Waypoint::Visited;
        info!(
            "{} reached {} at {:?}",
            robot_name,
            waypoint_name,
            clock.now()
        );
    }
}

/// Draws robots using a circle and arrow with gizmos.
fn draw_robots(robots: Query<&Transform, With<Robot>>, mut gizmos: Gizmos) {
    let robot_radius = 40.0;

    for transform in &robots {
        let position = transform.translation.truncate();
        gizmos.circle_2d(position, robot_radius / 2.0, Color::default());
        draw_direction_arrow(&mut gizmos, transform, robot_radius, Color::default());
    }
}

/// Draws waypoints using a circle and arrow with gizmos.
fn draw_waypoints(waypoints: Query<(&Transform, &Waypoint)>, mut gizmos: Gizmos) {
    let waypoint_size = 50.0;

    for (transform, waypoint) in &waypoints {
        let color = match waypoint {
            Waypoint::Visited => Color::from(basic::GREEN),
            Waypoint::Unvisited => Color::from(basic::GRAY),
        };
        gizmos.circle_2d(transform.translation.truncate(), waypoint_size / 2.0, color);
        draw_direction_arrow(&mut gizmos, transform, waypoint_size, color);
    }
}

/// Draws a direction arrow with gizmos.
fn draw_direction_arrow(gizmos: &mut Gizmos, transform: &Transform, length: f32, color: Color) {
    let position = transform.translation.truncate();
    let direction = (transform.rotation * Vec3::X).truncate();
    gizmos.arrow_2d(position, position + direction * length, color);
}

struct Playback {
    timer: Timer,
    step_idx: usize,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.1, TimerMode::Repeating),
            step_idx: 0,
        }
    }
}

// TODO: This functionality should be moved into the plugin.
fn simple_playback(world: &mut World, mut state: Local<Playback>) {
    state.timer.tick(world.resource::<Time>().delta());
    if !state.timer.just_finished() {
        return;
    }

    let simulation = world.query::<&Simulation>().single(world).unwrap();
    let Some(step) = simulation.steps().get(state.step_idx).cloned() else {
        info!("Restarting playback");
        state.step_idx = 0;
        // Init steps are used to reset the simulation to the initial state.
        for event in simulation.init_step().events.clone() {
            event.apply(world);
        }
        return;
    };
    state.step_idx += 1;

    // Apply events for changes in this simulation step to the main world.
    for event in step.events {
        event.apply(world);
    }
}

/// Generates a random transform within the given position bounds.
fn random_transform(rng: &mut impl Rng, position_bounds: Rect) -> Transform {
    Transform {
        translation: Vec3::new(
            rng.gen_range(position_bounds.min.x..=position_bounds.max.x),
            rng.gen_range(position_bounds.min.y..=position_bounds.max.y),
            0.0,
        ),
        rotation: Quat::from_rotation_z(rng.gen_range(0..4) as f32 * std::f32::consts::FRAC_PI_2),
        scale: Vec3::ONE,
    }
}
