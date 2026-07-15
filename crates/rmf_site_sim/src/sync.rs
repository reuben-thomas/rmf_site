use crate::compute::SimulationComputeClock;
use crate::event::DiscreteEvent;
use crate::simulation::{SimulationComputeUpdate, SimulationStep};
use crate::time::SimulationTime;
use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::ecs::system::Command;
use bevy::prelude::*;
use bevy::utils::synccell::SyncCell;
use crossbeam_channel::Sender;

/// Extracts entities, components, and resources from the main world into the simulation world.
#[derive(Default, Clone)]
pub struct Extractor {
    component_extractors: Vec<ComponentExtractor>,
    resource_extractors: Vec<ResourceExtractor>,
}

impl Extractor {
    // TODO: What if register_component is called twice for the same type?
    /// Registers a component to extract.
    pub fn register_component<T: Component + Clone>(&mut self) {
        self.component_extractors
            .push(ComponentExtractor::new::<T>());
    }

    /// Registers a resource to extract.
    pub fn register_resource<T: Resource + Clone>(&mut self) {
        self.resource_extractors.push(ResourceExtractor::new::<T>());
    }

    /// Extracts all entities that with at least one registered component.
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
    fn new<T: Component + Clone>() -> Self {
        Self {
            extract_entities: Self::extract_entities::<T>,
            extract_components: Self::extract_components::<T>,
        }
    }

    fn extract_entities<T: Component>(world: &World, entities: &mut EntityHashSet) {
        let Some(mut query) = world.try_query_filtered::<Entity, With<T>>() else {
            return;
        };
        entities.extend(query.iter(world));
    }

    fn extract_components<T: Component + Clone>(world: &World) -> Box<dyn DiscreteEvent> {
        let components = world
            .iter_entities()
            .filter_map(|entity| Some((entity.id(), entity.get::<T>().cloned()?)))
            .collect();
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
