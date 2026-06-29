use crate::extract::EntityExtractor;
use crate::simulation::SimulationStep;
use crate::time::SimulationTime;
use bevy::prelude::*;
use crossbeam_channel::Sender;

pub fn compute_simulation(
    mut startup_schedule: Schedule,
    mut compute_schedule: Schedule,
    extractor: EntityExtractor,
    entities: Vec<Entity>,
    initial_step: SimulationStep,
    step_sender: Sender<SimulationStep>,
) {
    let mut world = create_world(entities, initial_step);
    startup_schedule.run(&mut world);
    compute_schedule.run(&mut world);

    let time = SimulationTime::default();
    let commands = extractor.extract_components(&world);

    let _ = step_sender.send(SimulationStep { time, commands });
}

fn create_world(entities: Vec<Entity>, initial_step: SimulationStep) -> World {
    let mut world = World::new();
    for entity in entities {
        spawn_at(&mut world, entity);
    }
    for command in initial_step.commands {
        command.apply(&mut world);
    }
    world
}

// TODO: This method is only in newer versions of Bevy.
fn spawn_at(world: &mut World, entity: Entity) {
    #[allow(deprecated)]
    let _ = world.insert_or_spawn_batch([(entity, ())]);
}
