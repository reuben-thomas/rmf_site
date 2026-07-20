use crate::compute::SimulationComputeClock;
use crate::event::DiscreteEvent;
use crate::simulation::{SimulationComputeUpdate, SimulationStep};
use crate::time::SimulationTime;
use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::ecs::system::Command;
use bevy::prelude::*;
use bevy::utils::synccell::SyncCell;
use crossbeam_channel::Sender;
use std::any::TypeId;
use std::collections::HashSet;
use std::marker::PhantomData;

/// Extracts entities, components, and resources from the main world into the simulation world.
///
/// Only entities with the marker component `M` are extracted.
pub struct Extractor<M: Component> {
    component_extractors: Vec<ComponentExtractor>,
    resource_extractors: Vec<ResourceExtractor>,
    registered_component_type_ids: HashSet<TypeId>,
    registered_resource_type_ids: HashSet<TypeId>,
    marker: PhantomData<M>,
}

impl<M: Component> Default for Extractor<M> {
    fn default() -> Self {
        Self {
            component_extractors: Vec::new(),
            resource_extractors: Vec::new(),
            registered_component_type_ids: HashSet::new(),
            registered_resource_type_ids: HashSet::new(),
            marker: PhantomData,
        }
    }
}

impl<M: Component> Clone for Extractor<M> {
    fn clone(&self) -> Self {
        Self {
            component_extractors: self.component_extractors.clone(),
            resource_extractors: self.resource_extractors.clone(),
            registered_component_type_ids: self.registered_component_type_ids.clone(),
            registered_resource_type_ids: self.registered_resource_type_ids.clone(),
            marker: PhantomData,
        }
    }
}

impl<M: Component> Extractor<M> {
    /// Registers a component to extract.
    pub fn register_component<T: Component + Clone>(&mut self) {
        if !self.registered_component_type_ids.insert(TypeId::of::<T>()) {
            warn!(
                "Component {} is already registered for extraction.",
                std::any::type_name::<T>()
            );
            return;
        }

        self.component_extractors
            .push(ComponentExtractor::new::<T, M>());
    }

    /// Registers a resource to extract.
    pub fn register_resource<T: Resource + Clone>(&mut self) {
        if !self.registered_resource_type_ids.insert(TypeId::of::<T>()) {
            warn!(
                "Resource {} is already registered for extraction.",
                std::any::type_name::<T>()
            );
            return;
        }

        self.resource_extractors.push(ResourceExtractor::new::<T>());
    }

    /// Extracts all marked entities with at least one registered component.
    pub fn extract_entities(&self, world: &World) -> Vec<Entity> {
        let mut entities = EntityHashSet::default();
        for extractor in &self.component_extractors {
            (extractor.extract_entities)(world, &mut entities);
        }
        entities.into_iter().collect()
    }

    /// Creates the events that capture the extracted component and resource states.
    pub fn create_extract_events(&self, world: &World) -> Vec<Box<dyn DiscreteEvent>> {
        self.extract_components(world)
            .chain(self.extract_resources(world))
            .collect()
    }

    /// Extracts all components that should be synchronized.
    fn extract_components<'a>(
        &'a self,
        world: &'a World,
    ) -> impl Iterator<Item = Box<dyn DiscreteEvent>> + 'a {
        self.component_extractors
            .iter()
            .map(move |extractor| (extractor.extract_components)(world))
    }

    /// Extracts all resources that should be synchronized.
    fn extract_resources<'a>(
        &'a self,
        world: &'a World,
    ) -> impl Iterator<Item = Box<dyn DiscreteEvent>> + 'a {
        self.resource_extractors
            .iter()
            .filter_map(move |extractor| (extractor.extract_resource)(world))
    }
}

/// An extractor for a single component type.
#[derive(Clone, Copy)]
struct ComponentExtractor {
    extract_entities: fn(&World, &mut EntityHashSet),
    extract_components: fn(&World) -> Box<dyn DiscreteEvent>,
}

impl ComponentExtractor {
    fn new<T: Component + Clone, M: Component>() -> Self {
        Self {
            extract_entities: Self::extract_entities::<T, M>,
            extract_components: Self::extract_components::<T, M>,
        }
    }

    fn extract_entities<T: Component, M: Component>(world: &World, entities: &mut EntityHashSet) {
        let Some(mut query) = world.try_query_filtered::<Entity, (With<T>, With<M>)>() else {
            return;
        };
        entities.extend(query.iter(world));
    }

    fn extract_components<T: Component + Clone, M: Component>(
        world: &World,
    ) -> Box<dyn DiscreteEvent> {
        let components = world
            .try_query_filtered::<(Entity, &T), With<M>>()
            .map(|mut query| {
                query
                    .iter(world)
                    .map(|(entity, component)| (entity, component.clone()))
                    .collect()
            })
            .unwrap_or_default();
        Box::new(ExtractComponents::<T>(components))
    }
}

/// An extractor for a single resource type.
#[derive(Clone, Copy)]
struct ResourceExtractor {
    extract_resource: fn(&World) -> Option<Box<dyn DiscreteEvent>>,
}

impl ResourceExtractor {
    fn new<T: Resource + Clone>() -> Self {
        Self {
            extract_resource: Self::extract_resource::<T>,
        }
    }

    fn extract_resource<T: Resource + Clone>(world: &World) -> Option<Box<dyn DiscreteEvent>> {
        let value = world.get_resource::<T>()?.clone();
        Some(Box::new(ExtractResource(value)))
    }
}

#[derive(Clone)]
pub struct ExtractComponents<T: Component>(pub EntityHashMap<T>);

impl<T: Component + Clone> Command for ExtractComponents<T> {
    fn apply(self, world: &mut World) {
        for (entity, value) in self.0 {
            if let Ok(mut e) = world.get_entity_mut(entity) {
                e.insert(value);
            }
        }
    }
}

#[derive(Clone)]
pub struct ExtractResource<T: Resource>(pub T);

impl<T: Resource + Clone> Command for ExtractResource<T> {
    fn apply(self, world: &mut World) {
        world.insert_resource(self.0);
    }
}

#[derive(Resource)]
pub struct StateUpdateSender(Sender<SimulationComputeUpdate>);

impl StateUpdateSender {
    pub fn new(sender: Sender<SimulationComputeUpdate>) -> Self {
        Self(sender)
    }

    fn send(&self, time: SimulationTime, step: SimulationStep) {
        let _ = self.0.send(SimulationComputeUpdate::Step(time, step));
    }
}

/// A buffer to store the [`DiscreteEvent`]s executed in the current step,
/// sent to the main world as a single [`SimulationStep`].
#[derive(Resource)]
pub struct SimulationEventBuffer(SyncCell<Vec<Box<dyn DiscreteEvent>>>);

impl Default for SimulationEventBuffer {
    fn default() -> Self {
        Self(SyncCell::new(Vec::new()))
    }
}

impl SimulationEventBuffer {
    pub(crate) fn extend(&mut self, events: impl IntoIterator<Item = Box<dyn DiscreteEvent>>) {
        self.0.get().extend(events);
    }

    fn take(&mut self) -> Vec<Box<dyn DiscreteEvent>> {
        std::mem::take(self.0.get())
    }

    pub(crate) fn send_step(
        clock: Res<SimulationComputeClock>,
        mut buffer: ResMut<Self>,
        sender: Res<StateUpdateSender>,
    ) {
        let events = buffer.take();
        if events.is_empty() {
            return;
        }

        sender.send(clock.now(), SimulationStep { events });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compute::spawn_at;

    #[derive(Component, Clone)]
    struct Marker;

    #[derive(Component, Clone, Debug, PartialEq)]
    struct Value(u32);

    #[test]
    fn test_marker_component_filters_extraction() {
        let mut world = World::new();
        let marked = world.spawn((Marker, Value(1))).id();
        let unmarked = world.spawn(Value(2)).id();

        let mut extractor = Extractor::<Marker>::default();
        extractor.register_component::<Value>();

        // Create a simulation world with the extracted entities and components.
        let mut sim_world = World::new();
        let extract_entities = extractor.extract_entities(&world);
        for entity in extract_entities.clone() {
            spawn_at(&mut sim_world, entity);
        }
        let extract_events = extractor.create_extract_events(&world);
        for event in extract_events {
            event.apply(&mut sim_world);
        }

        assert_eq!(
            extract_entities,
            vec![marked],
            "Only the entity with the marker component should be extracted."
        );
        assert_eq!(
            sim_world.get::<Value>(marked),
            Some(&Value(1)),
            "Marked entity should be extracted with its Value component."
        );
        assert_eq!(
            sim_world.get::<Value>(unmarked),
            None,
            "Unmarked entity should not be extracted."
        );
    }

    #[test]
    fn test_duplicate_registration_ignored() {
        #[derive(Resource, Clone)]
        struct Config;

        let mut extractor = Extractor::<Marker>::default();
        for _ in 0..2 {
            extractor.register_component::<Value>();
            extractor.register_resource::<Config>();
        }

        assert_eq!(extractor.component_extractors.len(), 1);
        assert_eq!(extractor.resource_extractors.len(), 1);
    }
}
