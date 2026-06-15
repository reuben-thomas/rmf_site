use bevy::log::LogPlugin;
use bevy::prelude::*;
use rmf_site_sim::*;
use std::fmt::Debug;

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(LogPlugin::default())
        .add_plugins(SimulationPlugin)
        // Representation of a load balancer system:
        // client_generate --[ClientRequest]--> load_balancer --[Request]--> server --[Response]--> client_collect
        .register_discrete_event::<ClientRequest>()
        .register_discrete_event::<Request>()
        .register_discrete_event::<Response>()
        .add_systems(Startup, (setup, client_generate.after(setup)))
        .add_systems(Update, (load_balancer, server, client_collect))
        .add_systems(
            OnEnter(ComputeState::Complete),
            log_compute_results::<u32, RequestCount>,
        )
        .run();
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
    time: u32,
}

#[derive(Event)]
struct Request {
    request: String,
    source: Entity,
    target: Entity,
    time: u32,
}

#[derive(Event)]
struct Response {
    response: String,
    source: Entity,
    target: Entity,
    time: u32,
}

/// TODO: Remove after implementing macro
macro_rules! discrete_event {
    ($ty:ty) => {
        impl DiscreteEvent for $ty {
            type Time = u32;

            fn time(&self) -> u32 {
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

fn setup(mut commands: Commands) {
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
                time,
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
            "t={}: routed {:?} -> server #{server_idx} ({server})",
            req.time, req.request
        );

        count.0 += 1;
        server_idx = (server_idx + 1) % servers.len();

        writer.write(Request {
            request: req.request,
            source: req.target,
            target: server,
            time: req.time + 1,
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
            "t={}: server {} handling {:?}",
            req.time, req.target, req.request
        );

        if let Ok(mut count) = counts.get_mut(req.target) {
            count.0 += 1;
        }
        responses.write(Response {
            response: format!("echo from server: {}", req.request),
            source: req.target,
            target: req.source,
            time: req.time + 1,
        });
    }
}

fn log_compute_results<Time: SimTime, T: Component + Debug>(
    results: Option<Res<ComputeResults<Time, T>>>,
) {
    let Some(results) = results else {
        return;
    };
    for (time, result) in &results.results {
        for (entity, component) in &result.changes {
            info!("t={time:?}: {entity} -> {component:?}");
        }
    }
}

fn client_collect(
    mut responses: DiscreteEventReader<Response>,
    mut count: Query<&mut RequestCount, With<Client>>,
) {
    let mut count = count.single_mut().unwrap();
    for res in responses.read() {
        info!(
            "t={}: collected {:?} from server {}",
            res.time, res.response, res.source
        );
        count.0 += 1;
    }
}
