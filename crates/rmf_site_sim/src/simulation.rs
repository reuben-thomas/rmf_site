use bevy::ecs::component::ComponentId;
use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::*;
use std::collections::HashMap;

/// A unique simulation instance computed on a [`SimulationSet`].
#[derive(Component)]
pub struct Simulation {
    world: World,
}
