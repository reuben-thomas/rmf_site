use bevy::log::LogPlugin;
use bevy::prelude::*;
use rmf_site_sim::*;

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(LogPlugin::default())
        .add_plugins(SimulationPlugin)
        // Representation of a load balancer system:
        // client_generate --[ClientRequest]--> load_balancer --[Request]--> server --[Response]--> client_collect
        .add_discrete_event::<ClientRequest>()
        .add_discrete_event::<Request>()
        .add_discrete_event::<Response>()
        .add_systems(Startup, (setup, client_generate.after(setup)))
        .add_systems(Update, (load_balancer, server, client_collect))
        .run();
}

#[derive(Component)]
struct Client;

#[derive(Component)]
struct LoadBalancer;

#[derive(Component)]
struct Server;

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
    commands.spawn(Client);
    commands.spawn(LoadBalancer);
    for _ in 0..3 {
        commands.spawn(Server);
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
) {
    let servers: Vec<Entity> = servers.iter().collect();
    let mut server_idx = 0;

    for req in reader.read() {
        let server = servers[server_idx];

        info!(
            "t={}: routed {:?} -> server #{server_idx} ({server})",
            req.time, req.request
        );

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
) {
    for req in requests.read() {
        info!(
            "t={}: server {} handling {:?}",
            req.time, req.target, req.request
        );
        responses.write(Response {
            response: format!("echo from server: {}", req.request),
            source: req.target,
            target: req.source,
            time: req.time + 1,
        });
    }
}

fn client_collect(mut responses: DiscreteEventReader<Response>) {
    for res in responses.read() {
        info!(
            "t={}: collected {:?} from server {}",
            res.time, res.response, res.source
        );
    }
}
