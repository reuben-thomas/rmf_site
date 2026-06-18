use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use rmf_site_sim::*;
use std::fmt::Debug;
use std::time::Duration;

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
        .add_plugins(LogPlugin::default())
        .add_plugins(SimulationPlugin);
    // .add_systems(Startup, start_simulation);

    // client_generate --[ClientRequest]--> load_balancer --[Request]--> server --[Response]--> client_collect
    let simulation_set = SimulationSetBuilder::new(app.world_mut())
        .register_event::<ClientRequest>()
        .register_event::<Request>()
        .register_event::<Response>()
        .register_tracked_component::<RequestCount>()
        .add_startup_systems((spawn_entities, client_generate).chain())
        .add_compute_systems((load_balancer, server, client_collect))
        .build();
    app.world_mut().spawn(simulation_set);

    app.run();
}

#[derive(Component)]
struct Client;

#[derive(Component)]
struct LoadBalancer;

#[derive(Component)]
struct Server;

#[derive(Component, Debug, Clone, Default)]
struct RequestCount(u32);

#[derive(Event)]
struct ClientRequest {
    request: String,
    source: Entity,
    target: Entity,
}

#[derive(Event)]
struct Request {
    request: String,
    source: Entity,
    target: Entity,
}

#[derive(Event)]
struct Response {
    response: String,
    source: Entity,
    target: Entity,
}

fn spawn_entities(mut commands: Commands) {
    commands.spawn((Client, RequestCount::default()));
    commands.spawn((LoadBalancer, RequestCount::default()));
    for _ in 0..3 {
        commands.spawn((Server, RequestCount::default()));
    }
}

fn client_generate(
    mut requests: DiscreteEventWriter<ClientRequest>,
    clients: Query<Entity, With<Client>>,
    load_balancers: Query<Entity, With<LoadBalancer>>,
) {
    let client = clients.single().unwrap();
    let load_balancer = load_balancers.single().unwrap();

    for (concurrent_requests, time) in (0..5).enumerate() {
        for i in 0..concurrent_requests {
            requests.write(DiscreteEvent {
                time: SimTime::new(Duration::from_secs(time)),
                event: ClientRequest {
                    request: format!("request-{i}/{concurrent_requests}"),
                    source: client,
                    target: load_balancer,
                },
            });
        }
    }
}

fn load_balancer(
    mut reader: DiscreteEventReader<ClientRequest>,
    mut writer: DiscreteEventWriter<Request>,
    servers: Query<Entity, With<Server>>,
    mut count: Query<&mut RequestCount, With<LoadBalancer>>,
) {
    let servers: Vec<Entity> = servers.iter().collect();
    let mut server_idx = 0;
    let mut count = count.single_mut().unwrap();

    for req in reader.read() {
        let server = servers[server_idx];

        info!(
            "t={:?}: routed {:?} -> server #{server_idx} ({server})",
            req.time, req.event.request
        );

        count.0 += 1;
        server_idx = (server_idx + 1) % servers.len();

        writer.write(DiscreteEvent {
            time: SimTime::new(req.time.elapsed() + Duration::from_secs(1)),
            event: Request {
                request: req.event.request,
                source: req.event.target,
                target: server,
            },
        });
    }
}

fn server(
    mut requests: DiscreteEventReader<Request>,
    mut responses: DiscreteEventWriter<Response>,
    mut counts: Query<&mut RequestCount, With<Server>>,
) {
    for req in requests.read() {
        info!(
            "t={:?}: server {} handling {:?}",
            req.time, req.event.target, req.event.request
        );

        if let Ok(mut count) = counts.get_mut(req.event.target) {
            count.0 += 1;
        }
        responses.write(DiscreteEvent {
            time: SimTime::new(req.time.elapsed() + Duration::from_secs(1)),
            event: Response {
                response: format!("echo from server: {}", req.event.request),
                source: req.event.target,
                target: req.event.source,
            },
        });
    }
}

fn client_collect(
    mut responses: DiscreteEventReader<Response>,
    mut count: Query<&mut RequestCount, With<Client>>,
) {
    let mut count = count.single_mut().unwrap();
    for res in responses.read() {
        info!(
            "t={:?}: collected {:?} from server {}",
            res.time, res.event.response, res.event.source
        );
        count.0 += 1;
    }
}
