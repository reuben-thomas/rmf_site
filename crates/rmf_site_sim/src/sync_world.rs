use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;

pub fn sync(_main: &World, _sim: &mut World) {
    // spawn entities in target
    // clone entity values
}

#[derive(Default)]
pub struct EntityMap {
    main_to_sim: EntityHashMap<Entity>,
    sim_to_main: EntityHashMap<Entity>,
}

impl EntityMap {
    pub fn set_mapped(&mut self, main: Entity, sim: Entity) {
        self.main_to_sim.insert(main, sim);
        self.sim_to_main.insert(sim, main);
    }

    pub fn get_sim(&self, main: Entity) -> Option<Entity> {
        self.main_to_sim.get(&main).copied()
    }

    pub fn get_main(&self, sim: Entity) -> Option<Entity> {
        self.sim_to_main.get(&sim).copied()
    }

    pub fn remove_main(&mut self, main: Entity) -> Option<Entity> {
        let sim = self.main_to_sim.remove(&main)?;
        self.sim_to_main.remove(&sim);
        Some(sim)
    }

    pub fn remove_sim(&mut self, sim: Entity) -> Option<Entity> {
        let main = self.sim_to_main.remove(&sim)?;
        self.main_to_sim.remove(&main);
        Some(main)
    }
}
