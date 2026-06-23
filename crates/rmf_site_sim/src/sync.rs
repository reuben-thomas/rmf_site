use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use std::marker::PhantomData;

/// Clones all registered component types for all entities between two worlds.
#[derive(Default)]
pub struct EntityCloner {
    entity_map: EntityMap,
    cloners: Vec<Box<dyn ErasedComponentCloner>>,
}

impl EntityCloner {
    pub fn register<T: Component + Clone>(&mut self) {
        self.cloners.push(Box::new(ComponentCloner::<T>::default()));
    }

    pub fn clone_to_sim(&mut self, main_world: &World, sim_world: &mut World) {
        for cloner in &self.cloners {
            cloner.clone_to_sim(&mut self.entity_map, main_world, sim_world);
        }
    }

    pub fn clone_to_main(&mut self, sim_world: &World, main_world: &mut World) {
        for cloner in &self.cloners {
            cloner.clone_to_main(&mut self.entity_map, sim_world, main_world);
        }
    }
}

/// Maps entities between a simulation and main [`World`].
#[derive(Default)]
pub struct EntityMap {
    main_to_sim: EntityHashMap<Entity>,
    sim_to_main: EntityHashMap<Entity>,
}

impl EntityMap {
    fn set_mapped(&mut self, main: Entity, sim: Entity) {
        self.main_to_sim.insert(main, sim);
        self.sim_to_main.insert(sim, main);
    }

    fn get_or_spawn_sim(&mut self, main: Entity, sim_world: &mut World) -> Entity {
        self.main_to_sim.get(&main).copied().unwrap_or_else(|| {
            let sim = sim_world.spawn_empty().id();
            self.set_mapped(main, sim);
            sim
        })
    }

    fn get_or_spawn_main(&mut self, sim: Entity, main_world: &mut World) -> Entity {
        self.sim_to_main.get(&sim).copied().unwrap_or_else(|| {
            let main = main_world.spawn_empty().id();
            self.set_mapped(main, sim);
            main
        })
    }
}

pub struct ComponentCloner<T: Component + Clone> {
    _marker: PhantomData<T>,
}

impl<T: Component + Clone> Default for ComponentCloner<T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T: Component + Clone> ComponentCloner<T> {
    pub fn clone_to_sim(
        &self,
        entity_map: &mut EntityMap,
        main_world: &World,
        sim_world: &mut World,
    ) {
        for (main_entity, component) in collect_components::<T>(main_world) {
            let sim_entity = entity_map.get_or_spawn_sim(main_entity, sim_world);
            insert_component(sim_world, sim_entity, component);
        }
    }

    pub fn clone_to_main(
        &self,
        entity_map: &mut EntityMap,
        sim_world: &World,
        main_world: &mut World,
    ) {
        for (sim_entity, component) in collect_components::<T>(sim_world) {
            let main_entity = entity_map.get_or_spawn_main(sim_entity, main_world);
            insert_component(main_world, main_entity, component);
        }
    }
}

fn collect_components<T: Component + Clone>(world: &World) -> Vec<(Entity, T)> {
    world
        .iter_entities()
        .filter_map(|entity| Some((entity.id(), entity.get::<T>().cloned()?)))
        .collect()
}

fn insert_component<T: Component>(world: &mut World, entity: Entity, component: T) {
    if let Ok(mut e) = world.get_entity_mut(entity) {
        e.insert(component);
    }
}

trait ErasedComponentCloner {
    fn clone_to_sim(&self, entity_map: &mut EntityMap, main_world: &World, sim_world: &mut World);
    fn clone_to_main(&self, entity_map: &mut EntityMap, sim_world: &World, main_world: &mut World);
}

impl<T: Component + Clone> ErasedComponentCloner for ComponentCloner<T> {
    fn clone_to_sim(&self, entity_map: &mut EntityMap, main_world: &World, sim_world: &mut World) {
        self.clone_to_sim(entity_map, main_world, sim_world);
    }

    fn clone_to_main(&self, entity_map: &mut EntityMap, sim_world: &World, main_world: &mut World) {
        self.clone_to_main(entity_map, sim_world, main_world);
    }
}
