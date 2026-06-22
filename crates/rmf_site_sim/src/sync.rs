use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;

fn sync_to_sim(main_world: &mut World, sim_world: &mut World) {
    todo!();
}

/// Maps entities between a simulation and main ['World'].
#[derive(Default)]
#[allow(dead_code)]
struct EntityMap {
    main_to_sim: EntityHashMap<Entity>,
    sim_to_main: EntityHashMap<Entity>,
}

impl EntityMap {
    fn set_mapped(&mut self, main: Entity, sim: Entity) {
        self.main_to_sim.insert(main, sim);
        self.sim_to_main.insert(sim, main);
    }

    fn get_sim(&self, main: Entity) -> Option<Entity> {
        self.main_to_sim.get(&main).copied()
    }

    fn get_main(&self, sim: Entity) -> Option<Entity> {
        self.sim_to_main.get(&sim).copied()
    }

    fn remove_main(&mut self, main: Entity) -> Option<Entity> {
        let sim = self.main_to_sim.remove(&main)?;
        self.sim_to_main.remove(&sim);
        Some(sim)
    }

    fn remove_sim(&mut self, sim: Entity) -> Option<Entity> {
        let main = self.sim_to_main.remove(&sim)?;
        self.main_to_sim.remove(&main);
        Some(main)
    }
}
