//! In this example, we set up, compute, and animate a simple discrete event simulation for a robot fleet.
//! A planner receives requests, generates and sends trajectories for robots to reach a goal waypoint.
//! Upon reaching a goal waypoint, the waypoint is marked as visited.
//!
//!
//! ```
//! --[RequestEvent..]--> Planner --[RobotTrajectoryEvent..]--> Robot --> [GoalReachedEvent] --> Waypoint
//! ```

use bevy::color::palettes::basic;
use bevy::prelude::*;
use rmf_site_sim::compute::SimulationComputeClock;
use rmf_site_sim::event::{DiscreteEventReader, DiscreteEventWriter, DiscreteEvents};
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

/// A request sent to a planner that requests a robot to reach a goal waypoint.
#[derive(Event, Clone)]
struct RequestEvent {
    robot: Entity,
    goal: Entity,
}

/// A trajectory sent from the planner to a robot from its current position to the destination.
#[derive(Event, Clone)]
struct RobotTrajectoryEvent {
    robot: Entity,
    trajectory: Trajectory,
}

#[derive(Clone)]
struct Trajectory {
    points: Vec<TrajectoryPoint>,
}

#[derive(Clone)]
struct TrajectoryPoint {
    time: SimulationTime,
    transform: Transform,
}

/// An event sent from a robot to its goal waypoint upon reaching it.
#[derive(Event, Clone)]
struct GoalReachedEvent {
    robot: Entity,
    waypoint: Entity,
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, SimulationPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, (draw_robots, draw_waypoints))
        .run();
}

fn setup(world: &mut World) {
    world.spawn(Camera2d);

    spawn_robots_waypoints(world);

    let simulation = SimulationBuilder::new()
        // Untracked components may not change, or may not be useful for visualization.
        .register_untracked_component::<Robot>()
        .register_untracked_component::<Name>()
        // Changes to positions and waypoint states are relevant to the simulation and should be tracked.
        .register_tracked_component::<Transform>()
        .register_tracked_component::<Waypoint>()
        // Events used within this simulation, required for 'DiscreteEventReader<T>', and 'DiscreteEventWriter<T>' in systems to work.
        .register_discrete_event::<RequestEvent>()
        .register_discrete_event::<RobotTrajectoryEvent>()
        .register_discrete_event::<GoalReachedEvent>()
        // Systems representing models used to compute the simulation.
        .add_startup_systems(schedule_initial_requests)
        .add_compute_systems((planner, robot, waypoint))
        .build(world);

    world.spawn(simulation);
}

fn spawn_robots_waypoints(world: &mut World) {
    world.spawn((
        Robot,
        Name::new("RobotA".to_string()),
        Transform::from_xyz(-200.0, 0.0, 0.0),
    ));
    world.spawn((
        Waypoint::Visited,
        Name::new("WaypointA".to_string()),
        Transform::from_xyz(-200.0, 0.0, 0.0),
    ));
    world.spawn((
        Waypoint::Unvisited,
        Name::new("WaypointB".to_string()),
        Transform::from_xyz(200.0, 0.0, 0.0),
    ));
}

fn schedule_initial_requests(
    mut writer: DiscreteEventWriter<RequestEvent>,
    robots: Query<(Entity, &Name), With<Robot>>,
    waypoints: Query<(Entity, &Name, &Waypoint)>,
) {
    let mut schedule_time_secs = 1;

    for (robot_entity, robot_name) in robots.iter() {
        for (waypoint_entity, waypoint_name, waypoint) in waypoints.iter() {
            if *waypoint == Waypoint::Visited {
                continue;
            }

            writer.schedule(
                SimulationTime::new(Duration::from_secs(schedule_time_secs)),
                RequestEvent {
                    robot: robot_entity,
                    goal: waypoint_entity,
                },
            );
            info!("Scheduled request for {} to {}", robot_name, waypoint_name);
            schedule_time_secs += 1;
            break;
        }
    }
}

fn planner(
    reader: DiscreteEventReader<RequestEvent>,
    robots: Query<(&Transform, &Name), With<Robot>>,
    goals: Query<&Transform, With<Waypoint>>,
    mut writer: DiscreteEventWriter<RobotTrajectoryEvent>,
    clock: Res<SimulationComputeClock>,
) {
    let planning_duration = Duration::from_secs(2);
    let planned_time = clock.now();

    for event in reader.read() {
        let (current_transform, robot_name) = robots.get(event.robot).unwrap();
        let target_transform = goals.get(event.goal).unwrap();
        let planned_time = SimulationTime::new(planned_time.elapsed() + planning_duration);
        info!(
            "Planned trajectory for {} at {:?}",
            robot_name, planned_time
        );

        writer.schedule(
            planned_time,
            RobotTrajectoryEvent {
                robot: event.robot,
                trajectory: Trajectory {
                    points: vec![
                        TrajectoryPoint {
                            time: planned_time,
                            transform: *current_transform,
                        },
                        TrajectoryPoint {
                            time: SimulationTime::new(
                                planned_time.elapsed() + Duration::from_secs(10),
                            ),
                            transform: *target_transform,
                        },
                    ],
                },
            },
        );
    }
}

fn robot(
    reader: DiscreteEventReader<RobotTrajectoryEvent>,
    mut robots: Query<&mut Transform, With<Robot>>,
    waypoints: Query<(Entity, &Transform), (With<Waypoint>, Without<Robot>)>,
    mut goal_writer: DiscreteEventWriter<GoalReachedEvent>,
) {
    for event in reader.read() {
        let trajectory_end = event.trajectory.points.last().unwrap();
        let mut robot_transform = robots.get_mut(event.robot).unwrap();
        *robot_transform = trajectory_end.transform;

        let reached = waypoints.iter().find(|(_, waypoint_transform)| {
            waypoint_transform
                .translation
                .distance(trajectory_end.transform.translation)
                == 0.0
        });
        if let Some((waypoint, _)) = reached {
            goal_writer.schedule(
                trajectory_end.time,
                GoalReachedEvent {
                    robot: event.robot,
                    waypoint,
                },
            );
        }
    }
}

fn waypoint(
    reader: DiscreteEventReader<GoalReachedEvent>,
    robots: Query<&Name, With<Robot>>,
    mut waypoints: Query<(&Name, &mut Waypoint), Without<Robot>>,
    clock: Res<SimulationComputeClock>,
) {
    for event in reader.read() {
        let robot_name = robots.get(event.robot).unwrap();
        let (waypoint_name, mut waypoint) = waypoints.get_mut(event.waypoint).unwrap();
        *waypoint = Waypoint::Visited;
        info!(
            "{} reached {} at {:?}",
            robot_name,
            waypoint_name,
            clock.now()
        );
    }
}

fn draw_robots(robots: Query<&Transform, With<Robot>>, mut gizmos: Gizmos) {
    let robot_radius = 40.0;

    for transform in &robots {
        gizmos.circle_2d(
            transform.translation.truncate(),
            robot_radius / 2.0,
            Color::default(),
        );
    }
}

fn draw_waypoints(waypoints: Query<(&Transform, &Waypoint)>, mut gizmos: Gizmos) {
    let waypoint_size = 50.0;

    for (transform, waypoint) in &waypoints {
        let color = match waypoint {
            Waypoint::Visited => Color::from(basic::GREEN),
            Waypoint::Unvisited => Color::from(basic::GRAY),
        };
        gizmos.rect_2d(
            transform.translation.truncate(),
            Vec2::splat(waypoint_size),
            color,
        );
    }
}
