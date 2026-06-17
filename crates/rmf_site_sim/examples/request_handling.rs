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
        .add_plugins(SimulationPlugin)
        .add_systems(Startup, (spawn_entities, start_simulation));

    // client_generate --[ClientRequest]--> load_balancer --[Request]--> server --[Response]--> client_collect
    let simulation_set = SimulationSetBuilder::new(app.world_mut())
        .register_event::<ClientRequest>()
        .register_event::<Request>()
        .register_event::<Response>()
        .register_tracked_component::<RequestCount>()
        .add_setup_systems(client_generate)
        .add_systems((load_balancer, server, client_collect))
        .build();
    app.world_mut().spawn(simulation_set);

    app.run();
}

fn start_simulation(mut writer: EventWriter<ComputeSimulation>) {
    writer.write(ComputeSimulation);
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
    time: SimTime,
}

#[derive(Event)]
struct Request {
    request: String,
    source: Entity,
    target: Entity,
    time: SimTime,
}

#[derive(Event)]
struct Response {
    response: String,
    source: Entity,
    target: Entity,
    time: SimTime,
}

/// TODO: Remove after implementing macro
macro_rules! discrete_event {
    ($ty:ty) => {
        impl DiscreteEvent for $ty {
            fn time(&self) -> SimTime {
                self.time
            }
            fn event_source(&self) -> Entity {
                self.source
            }
            fn event_target(&self) -> Entity {
                self.target
            }
        }
    };
}

discrete_event!(ClientRequest);
discrete_event!(Request);
discrete_event!(Response);

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
            requests.write(ClientRequest {
                request: format!("request-{i}/{concurrent_requests}"),
                source: client,
                target: load_balancer,
                time: SimTime::new(Duration::from_secs(time)),
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
            req.time, req.request
        );

        count.0 += 1;
        server_idx = (server_idx + 1) % servers.len();

        writer.write(Request {
            request: req.request,
            source: req.target,
            target: server,
            time: SimTime::new(req.time.elapsed() + Duration::from_secs(1)),
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
            req.time, req.target, req.request
        );

        if let Ok(mut count) = counts.get_mut(req.target) {
            count.0 += 1;
        }
        responses.write(Response {
            response: format!("echo from server: {}", req.request),
            source: req.target,
            target: req.source,
            time: SimTime::new(req.time.elapsed() + Duration::from_secs(1)),
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
            res.time, res.response, res.source
        );
        count.0 += 1;
    }
}
