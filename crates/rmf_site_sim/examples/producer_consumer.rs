use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use rmf_site_sim::*;
use std::time::Duration;

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
        .add_plugins(LogPlugin::default())
        .add_plugins(SimulationPlugin);

    // producer --[Message]--> consumer
    let simulation_set = SimulationSetBuilder::new(app.world_mut())
        .register_event::<Message>()
        .add_startup_systems((spawn_entities, produce).chain())
        .add_compute_systems(consume)
        .build();
    app.world_mut().spawn(simulation_set);

    app.run();
}

#[derive(Component)]
struct Producer;

#[derive(Component)]
struct Consumer;

#[derive(Event)]
struct Message {
    content: String,
}

fn spawn_entities(mut commands: Commands) {
    commands.spawn(Producer);
    commands.spawn(Consumer);
}

fn produce(
    mut writer: DiscreteEventWriter<Message>,
    producers: Query<Entity, With<Producer>>,
    consumers: Query<Entity, With<Consumer>>,
) {
    let producer = producers.single().unwrap();
    let consumer = consumers.single().unwrap();

    for i in 0..5 {
        writer.write(DiscreteEvent {
            time: SimTime::new(Duration::from_secs(i)),
            event: Message {
                content: format!("message-{i} sent"),
            },
        });
    }
}

fn consume(mut reader: DiscreteEventReader<Message>) {
    for msg in reader.read() {
        info!("t={:?}: received {:?}", msg.time, msg.event.content);
    }
}
