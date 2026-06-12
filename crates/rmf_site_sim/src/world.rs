use bevy::prelude::*;

pub struct SimWorld(pub World);

// TODO:
// - Introduce a function to take a snapshot of the present world, but only for a subset of entities and components
// - Handle mapping between both world and simulationworld
