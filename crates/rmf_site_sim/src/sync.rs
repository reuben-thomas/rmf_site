use crate::compute::SimulationComputeClock;
use crate::event::DynDiscreteEvent;
use crate::simulation::{SimulationComputeUpdate, SimulationStep};
use crate::time::SimulationTime;
use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use bevy::utils::TypeIdMap;
use crossbeam_channel::Sender;
use std::any::TypeId;

/// Synchronizes entities, components, and resources from a source world into a
/// target world.
#[derive(Clone, Default)]
pub struct Synchronizer {
    component_synchronizers: TypeIdMap<fn(&World, &mut World, Option<ComponentId>)>,
    resource_synchronizers: TypeIdMap<fn(&World, &mut World)>,
}

impl Synchronizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_component<T: Component + Clone>(&mut self) {
        if self
            .component_synchronizers
            .insert(TypeId::of::<T>(), Self::sync_components::<T>)
            .is_some()
        {
            warn!(
                "Component {} is already registered for synchronization.",
                std::any::type_name::<T>()
            );
        }
    }

    /// Registers a resource to synchronize.
    pub fn register_resource<T: Resource + Clone>(&mut self) {
        if self
            .resource_synchronizers
            .insert(TypeId::of::<T>(), Self::sync_resource::<T>)
            .is_some()
        {
            warn!(
                "Resource {} is already registered for synchronization.",
                std::any::type_name::<T>()
            );
        }
    }

    /// Synchronizes all entities that have at least one registered component.
    pub fn sync(&self, source: &World, target: &mut World) {
        self.sync_components_resources(source, target, None);
    }

    /// Synchronizes all entities that have the marker component `M`.
    pub fn sync_with<M: Component>(&self, source: &World, target: &mut World) {
        let Some(marker_id) = source.components().get_id(TypeId::of::<M>()) else {
            return;
        };
        self.sync_components_resources(source, target, Some(marker_id));
    }

    fn sync_components_resources(
        &self,
        source: &World,
        target: &mut World,
        marker: Option<ComponentId>,
    ) {
        for sync in self.component_synchronizers.values() {
            sync(source, target, marker);
        }
        for sync in self.resource_synchronizers.values() {
            sync(source, target);
        }
    }

    fn sync_components<T: Component + Clone>(
        source: &World,
        target: &mut World,
        marker: Option<ComponentId>,
    ) {
        let Some(mut query) = source.try_query::<(Entity, &T)>() else {
            return;
        };
        for (entity, component) in query.iter(source) {
            // Entities from the query are alive in `source`, so `entity()` cannot panic.
            if marker.is_none_or(|id| source.entity(entity).contains_id(id)) {
                spawn_at(target, entity);
                target.entity_mut(entity).insert(component.clone());
            }
        }
    }

    fn sync_resource<T: Resource + Clone>(source: &World, target: &mut World) {
        if let Some(value) = source.get_resource::<T>() {
            target.insert_resource(value.clone());
        }
    }
}

/// A snapshot of a world's synchronized entities, components, and resources.
pub struct SimulationState(pub World);

impl SimulationState {
    /// Extracts a state containing all entities with registered components.
    pub fn extract(synchronizer: &Synchronizer, source: &World) -> Self {
        let mut world = World::new();
        synchronizer.sync(source, &mut world);
        Self(world)
    }

    /// Extracts a state containing only entities with the marker component `M`.
    pub fn extract_with<M: Component>(synchronizer: &Synchronizer, source: &World) -> Self {
        let mut world = World::new();
        synchronizer.sync_with::<M>(source, &mut world);
        Self(world)
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

/// A buffer to store the [`DiscreteEvent`](crate::event::DiscreteEvent)s
/// executed in the current step, sent to the main world as a single
/// [`SimulationStep`].
#[derive(Resource, Default)]
pub struct SimulationEventBuffer(Vec<Box<dyn DynDiscreteEvent>>);

impl SimulationEventBuffer {
    pub(crate) fn extend(&mut self, events: impl IntoIterator<Item = Box<dyn DynDiscreteEvent>>) {
        self.0.extend(events);
    }

    fn take(&mut self) -> Vec<Box<dyn DynDiscreteEvent>> {
        std::mem::take(&mut self.0)
    }

    pub fn send_step(
        clock: Res<SimulationComputeClock>,
        mut buffer: ResMut<Self>,
        sender: Res<StateUpdateSender>,
    ) {
        let events = buffer.take();
        if events.is_empty() {
            return;
        }

        sender.send(clock.now(), SimulationStep::new(events));
    }
}

// TODO(@reuben-thomas): This method is only in newer versions of Bevy, this is
// a workaround. https://docs.rs/bevy/latest/bevy/prelude/struct.World.html#method.spawn_at
pub fn spawn_at(world: &mut World, entity: Entity) {
    #[allow(deprecated)]
    let _ = world.insert_or_spawn_batch([(entity, ())]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Clone)]
    struct Marker;

    #[derive(Component, Clone, Debug, PartialEq)]
    struct Value(u32);

    fn create_test_synchronizer() -> Synchronizer {
        let mut synchronizer = Synchronizer::new();
        synchronizer.register_component::<Marker>();
        synchronizer.register_component::<Value>();
        synchronizer
    }

    #[test]
    fn test_sync() {
        let mut source = World::new();
        let marked = source.spawn((Marker, Value(1))).id();
        let unmarked = source.spawn(Value(2)).id();

        let synchronizer = create_test_synchronizer();
        let mut target = World::new();
        synchronizer.sync(&source, &mut target);

        assert_eq!(
            vec![target.get::<Value>(marked), target.get::<Value>(unmarked)],
            vec![Some(&Value(1)), Some(&Value(2))],
            "All entities with registered components should be synchronized."
        );
    }

    #[test]
    fn test_sync_with() {
        let mut source = World::new();
        let marked = source.spawn((Marker, Value(1))).id();
        let unmarked = source.spawn(Value(2)).id();

        let synchronizer = create_test_synchronizer();
        let mut target = World::new();
        synchronizer.sync_with::<Marker>(&source, &mut target);

        assert_eq!(
            target.get::<Value>(marked),
            Some(&Value(1)),
            "An entity with marker component, and its registered components should be synchronized."
        );
        assert_eq!(
            target.get::<Value>(unmarked),
            None,
            "An entity without a marker component should not be synchronized."
        );
    }

    #[test]
    fn test_duplicate_registration_ignored() {
        #[derive(Resource, Clone)]
        struct Config;

        let mut synchronizer = Synchronizer::new();
        synchronizer.register_component::<Marker>();
        for _ in 0..2 {
            synchronizer.register_component::<Value>();
            synchronizer.register_resource::<Config>();
        }

        assert_eq!(synchronizer.component_synchronizers.len(), 2);
        assert_eq!(synchronizer.resource_synchronizers.len(), 1);
    }

    #[test]
    fn test_synchronization_round_trip() {
        let mut source = World::new();
        let marked = source.spawn((Marker, Value(1))).id();
        source.spawn(Value(2));

        let synchronizer = create_test_synchronizer();

        let state = SimulationState::extract_with::<Marker>(&synchronizer, &source);

        let mut target = World::new();
        synchronizer.sync(&state.0, &mut target);

        assert_eq!(target.get::<Value>(marked), Some(&Value(1)));
        assert_eq!(
            target.iter_entities().count(),
            1,
            "Only the marked entity should survive the round trip."
        );
    }
}
