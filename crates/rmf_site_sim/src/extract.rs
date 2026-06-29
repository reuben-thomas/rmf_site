use crate::simulation::{ComponentChanges, SimulationCommand};
use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::prelude::*;

#[derive(Default, Clone)]
pub struct EntityExtractor {
    extractors: Vec<ComponentExtractor>,
}

impl EntityExtractor {
    pub fn register<T: Component + Clone>(&mut self) {
        self.extractors.push(ComponentExtractor {
            extract_entities: |world, entities| {
                entities.extend(
                    world
                        .iter_entities()
                        .filter(|entity| entity.contains::<T>())
                        .map(|entity| entity.id()),
                );
            },
            extract_components: |world| {
                Box::new(ComponentChanges::<T>(collect_components::<T>(world)))
            },
        });
    }

    pub fn extract_entities(&self, world: &World) -> Vec<Entity> {
        let mut entities = EntityHashSet::default();
        for extractor in &self.extractors {
            (extractor.extract_entities)(world, &mut entities);
        }
        entities.into_iter().collect()
    }

    pub fn extract_components(&self, world: &World) -> Vec<Box<dyn SimulationCommand>> {
        self.extractors
            .iter()
            .map(|extractor| (extractor.extract_components)(world))
            .collect()
    }
}

fn collect_components<T: Component + Clone>(world: &World) -> EntityHashMap<T> {
    world
        .iter_entities()
        .filter_map(|entity| Some((entity.id(), entity.get::<T>().cloned()?)))
        .collect()
}

#[derive(Clone, Copy)]
struct ComponentExtractor {
    extract_entities: fn(&World, &mut EntityHashSet),
    extract_components: fn(&World) -> Box<dyn SimulationCommand>,
}
