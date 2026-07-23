use crate::compute::SimulationComputeClock;
use crate::event::DiscreteEvent;
use crate::simulation::{SimulationComputeUpdate, SimulationState, SimulationStep};
use crate::time::SimulationTime;
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use crossbeam_channel::Sender;
use std::any::TypeId;

/// Synchronizes entities, components, and resources from a source world into a target world.
#[derive(Clone)]
pub struct Synchronizer {
    component_synchronizers: Vec<ComponentSynchronizer>,
    resource_synchronizers: Vec<ResourceSynchronizer>,
}

impl Synchronizer {
    pub fn new<M: Component + Clone>() -> Self {
        Self {
            component_synchronizers: vec![ComponentSynchronizer::new::<M, M>()],
            resource_synchronizers: Vec::new(),
        }
    }

    pub fn register_component<T: Component + Clone, M: Component>(&mut self) {
        let type_id = TypeId::of::<T>();
        if self
            .component_synchronizers
            .iter()
            .any(|s| s.type_id == type_id)
        {
            warn!(
                "Component {} is already registered for synchronization.",
                std::any::type_name::<T>()
            );
            return;
        }

        self.component_synchronizers
            .push(ComponentSynchronizer::new::<T, M>());
    }

    /// Registers a resource to synchronize.
    pub fn register_resource<T: Resource + Clone>(&mut self) {
        let type_id = TypeId::of::<T>();
        if self
            .resource_synchronizers
            .iter()
            .any(|s| s.type_id == type_id)
        {
            warn!(
                "Resource {} is already registered for synchronization.",
                std::any::type_name::<T>()
            );
            return;
        }

        self.resource_synchronizers
            .push(ResourceSynchronizer::new::<T>());
    }

    fn synchronized_entities(&self, source: &World) -> Vec<Entity> {
        let mut entities = EntityHashSet::default();
        for synchronizer in &self.component_synchronizers {
            (synchronizer.sync_entities)(source, &mut entities);
        }
        entities.into_iter().collect()
    }

    pub fn sync(&self, source: &World, target: &mut World) {
        for entity in self.synchronized_entities(source) {
            spawn_at(target, entity);
        }
        for synchronizer in &self.component_synchronizers {
            (synchronizer.sync_components)(source, target);
        }
        for synchronizer in &self.resource_synchronizers {
            (synchronizer.sync_resource)(source, target);
        }
    }

    pub fn create_state(&self, source: &World) -> SimulationState {
        let mut state = World::new();
        self.sync(source, &mut state);
        SimulationState(state)
    }
}

/// Synchronizes a single component type.
#[derive(Clone, Copy)]
struct ComponentSynchronizer {
    type_id: TypeId,
    sync_entities: fn(&World, &mut EntityHashSet),
    sync_components: fn(&World, &mut World),
}

impl ComponentSynchronizer {
    fn new<T: Component + Clone, M: Component>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            sync_entities: Self::sync_entities::<T, M>,
            sync_components: Self::sync_components::<T, M>,
        }
    }

    fn sync_entities<T: Component, M: Component>(source: &World, entities: &mut EntityHashSet) {
        let Some(mut query) = source.try_query_filtered::<Entity, (With<T>, With<M>)>() else {
            return;
        };
        entities.extend(query.iter(source));
    }

    fn sync_components<T: Component + Clone, M: Component>(source: &World, target: &mut World) {
        let Some(mut query) = source.try_query_filtered::<(Entity, &T), With<M>>() else {
            return;
        };
        for (entity, component) in query.iter(source) {
            if let Ok(mut e) = target.get_entity_mut(entity) {
                e.insert(component.clone());
            }
        }
    }
}

/// Synchronizes a single resource type.
#[derive(Clone, Copy)]
struct ResourceSynchronizer {
    type_id: TypeId,
    sync_resource: fn(&World, &mut World),
}

impl ResourceSynchronizer {
    fn new<T: Resource + Clone>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            sync_resource: Self::sync_resource::<T>,
        }
    }

    fn sync_resource<T: Resource + Clone>(source: &World, target: &mut World) {
        if let Some(value) = source.get_resource::<T>() {
            target.insert_resource(value.clone());
        }
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
#[derive(Resource, Default)]
pub struct SimulationEventBuffer(Vec<Box<dyn DiscreteEvent>>);

impl SimulationEventBuffer {
    pub(crate) fn extend(&mut self, events: impl IntoIterator<Item = Box<dyn DiscreteEvent>>) {
        self.0.extend(events);
    }

    fn take(&mut self) -> Vec<Box<dyn DiscreteEvent>> {
        std::mem::take(&mut self.0)
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

    #[derive(Component, Clone)]
    struct Marker;

    #[derive(Component, Clone, Debug, PartialEq)]
    struct Value(u32);

    #[test]
    fn test_marker_component_filters_synchronization() {
        let mut source = World::new();
        let marked = source.spawn((Marker, Value(1))).id();
        let unmarked = source.spawn(Value(2)).id();

        let mut synchronizer = Synchronizer::new::<Marker>();
        synchronizer.register_component::<Value, Marker>();

        let source_entities = synchronizer.synchronized_entities(&source);

        let mut target = World::new();
        synchronizer.sync(&source, &mut target);

        assert_eq!(
            source_entities,
            vec![marked],
            "Only the entity with the marker component should be synchronized."
        );
        assert_eq!(
            target.get::<Value>(marked),
            Some(&Value(1)),
            "Marked entity should be synchronized with its Value component."
        );
        assert_eq!(
            target.get::<Value>(unmarked),
            None,
            "Unmarked entity should not be synchronized."
        );
    }

    #[test]
    fn test_duplicate_registration_ignored() {
        #[derive(Resource, Clone)]
        struct Config;

        let mut synchronizer = Synchronizer::new::<Marker>();
        for _ in 0..2 {
            synchronizer.register_component::<Value, Marker>();
            synchronizer.register_resource::<Config>();
        }

        assert_eq!(
            synchronizer.component_synchronizers.len(),
            2,
            "The marker component itself is auto-registered, plus Value."
        );
        assert_eq!(synchronizer.resource_synchronizers.len(), 1);
    }

    #[test]
    fn test_synchronization_round_trip() {
        let mut source = World::new();
        let marked = source.spawn((Marker, Value(1))).id();

        let mut synchronizer = Synchronizer::new::<Marker>();
        synchronizer.register_component::<Value, Marker>();

        let state = synchronizer.create_state(&source);

        let mut target = World::new();
        synchronizer.sync(&state.0, &mut target);

        assert_eq!(target.get::<Value>(marked), Some(&Value(1)),);
    }
}

// TODO: This method is only in newer versions of Bevy, this is a workaround.
// https://docs.rs/bevy/latest/bevy/prelude/struct.World.html#method.spawn_at
pub fn spawn_at(world: &mut World, entity: Entity) {
    #[allow(deprecated)]
    let _ = world.insert_or_spawn_batch([(entity, ())]);
}
