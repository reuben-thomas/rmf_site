use crate::compute::SimulationComputeClock;
use crate::event::DiscreteEvent;
use crate::schedule::SimulationComputeSet;
use crate::simulation::SimulationStep;
use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use crossbeam_channel::Sender;

/// Synchronizes entities, components, and resources between the main and simulation world.
///
/// All registererd resources and components, as well as their corresponding entities are extracted into the simulation world.
/// Only tracked components and resources are tracked for changes and sent as a [`SimulationStep`] to the main world.
#[derive(Default, Clone)]
pub struct Synchronizer {
    component_synchronizers: Vec<ComponentSynchronizer>,
    resource_synchronizers: Vec<ResourceSynchronizer>,
}

impl Synchronizer {
    /// Registers a component to extract without tracking changes.
    pub fn register_untracked<T: Component + Clone>(&mut self) {
        self.register_component::<T, false>();
    }
    /// Registers a component to extract and track changes.
    pub fn register_tracked<T: Component + Clone>(&mut self) {
        self.register_component::<T, true>();
    }

    // TODO: What if register_component is called twice for the same type?
    fn register_component<T: Component + Clone, const TRACKED: bool>(&mut self) {
        self.component_synchronizers
            .push(ComponentSynchronizer::new::<T, TRACKED>());
    }

    /// Registers a resource to extract without tracking changes.
    pub fn register_untracked_resource<T: Resource + Clone>(&mut self) {
        self.register_resource::<T, false>();
    }

    /// Registers a resource to extract and track changes.
    pub fn register_tracked_resource<T: Resource + Clone>(&mut self) {
        self.register_resource::<T, true>();
    }

    fn register_resource<T: Resource + Clone, const TRACKED: bool>(&mut self) {
        self.resource_synchronizers
            .push(ResourceSynchronizer::new::<T, TRACKED>());
    }

    /// Extracts all entities that should be synchronized.
    pub fn extract_entities(&self, world: &World) -> Vec<Entity> {
        let mut entities = EntityHashSet::default();
        for synchronizer in &self.component_synchronizers {
            (synchronizer.extract_entities)(world, &mut entities);
        }
        entities.into_iter().collect()
    }

    /// Extracts all components that should be synchronized.
    pub fn extract_components(&self, world: &World) -> Vec<Box<dyn DiscreteEvent>> {
        self.component_synchronizers
            .iter()
            .map(|synchronizer| (synchronizer.extract_components)(world))
            .collect()
    }

    /// Extracts all resources that should be synchronized.
    pub fn extract_resources(&self, world: &World) -> Vec<Box<dyn DiscreteEvent>> {
        self.resource_synchronizers
            .iter()
            .filter_map(|synchronizer| (synchronizer.extract_resource)(world))
            .collect()
    }

    /// Inserts the resources required for tracking changes to components.
    pub fn configure_tracking(
        &self,
        world: &mut World,
        schedule: &mut Schedule,
        step_sender: Sender<SimulationStep>,
    ) {
        world.init_resource::<SimulationStepBuffer>();
        world.insert_resource(StepSender::new(step_sender));

        for synchronizer in &self.component_synchronizers {
            if let Some(tracking_system) = synchronizer.tracking_system {
                schedule.add_systems(
                    tracking_system()
                        .before(SimulationStepBuffer::send_step)
                        .in_set(SimulationComputeSet::SendSimulationStep),
                );
            }
        }
        for synchronizer in &self.resource_synchronizers {
            if let Some(tracking_system) = synchronizer.tracking_system {
                schedule.add_systems(
                    tracking_system()
                        .before(SimulationStepBuffer::send_step)
                        .in_set(SimulationComputeSet::SendSimulationStep),
                );
            }
        }
        schedule.add_systems(
            SimulationStepBuffer::send_step
                .before(SimulationComputeClock::advance)
                .in_set(SimulationComputeSet::SendSimulationStep),
        );
    }
}

/// A synchronizer for a single component type.
#[derive(Clone, Copy)]
struct ComponentSynchronizer {
    extract_entities: fn(&World, &mut EntityHashSet),
    extract_components: fn(&World) -> Box<dyn DiscreteEvent>,
    tracking_system: Option<fn() -> ScheduleConfigs<ScheduleSystem>>,
}

impl ComponentSynchronizer {
    fn new<T: Component + Clone, const TRACKED: bool>() -> Self {
        Self {
            extract_entities: |world, entities| {
                entities.extend(
                    world
                        .iter_entities()
                        .filter(|entity| entity.contains::<T>())
                        .map(|entity| entity.id()),
                );
            },
            extract_components: |world| {
                let changes = world
                    .iter_entities()
                    .filter_map(|entity| Some((entity.id(), entity.get::<T>().cloned()?)))
                    .collect();
                Box::new(ComponentChanges::<T>(changes))
            },
            // TODO: More idiomatic way to do constexpr?
            tracking_system: if TRACKED {
                Some(|| SimulationStepBuffer::buffer_component_changes::<T>.into_configs())
            } else {
                None
            },
        }
    }
}

/// A synchronizer for a single resource type.
#[derive(Clone, Copy)]
struct ResourceSynchronizer {
    extract_resource: fn(&World) -> Option<Box<dyn DiscreteEvent>>,
    tracking_system: Option<fn() -> ScheduleConfigs<ScheduleSystem>>,
}

impl ResourceSynchronizer {
    fn new<T: Resource + Clone, const TRACKED: bool>() -> Self {
        Self {
            extract_resource: |world| {
                let value = world.get_resource::<T>()?.clone();
                Some(Box::new(ResourceChanges(value)))
            },
            tracking_system: if TRACKED {
                Some(|| SimulationStepBuffer::buffer_resource_changes::<T>.into_configs())
            } else {
                None
            },
        }
    }
}

#[derive(Resource)]
pub struct StepSender(Sender<SimulationStep>);

impl StepSender {
    pub fn new(sender: Sender<SimulationStep>) -> Self {
        Self(sender)
    }

    fn send(&self, step: SimulationStep) {
        // TODO: Error handling
        let _ = self.0.send(step);
    }
}

// TODO: Can we modify [`SystemBuffer`] for this instead?
/// A buffer to store a series of [`DiscreteEvent`]s built in the current step,
/// sent to the main world as a single [`SimulationStep`].
#[derive(Resource, Default)]
pub struct SimulationStepBuffer(Vec<Box<dyn DiscreteEvent>>);

impl SimulationStepBuffer {
    fn push(&mut self, event: Box<dyn DiscreteEvent>) {
        self.0.push(event);
    }

    fn take(&mut self) -> Vec<Box<dyn DiscreteEvent>> {
        std::mem::take(&mut self.0)
    }

    // TODO:
    // - verify change with partialeq to avoid mistaken triggers from `Query.mut()`
    // - store a full snapshot every x steps
    fn buffer_component_changes<T: Component + Clone>(
        changed: Query<(Entity, &T), Changed<T>>,
        mut buffer: ResMut<Self>,
    ) {
        let mut changes = EntityHashMap::default();
        for (entity, component) in changed.iter() {
            changes.insert(entity, component.clone());
        }
        if changes.is_empty() {
            return;
        }

        buffer.push(Box::new(ComponentChanges(changes)));
    }

    fn buffer_resource_changes<T: Resource + Clone>(
        resource: Option<Res<T>>,
        mut buffer: ResMut<Self>,
    ) {
        let Some(resource) = resource else {
            return;
        };
        if !resource.is_changed() {
            return;
        }

        buffer.push(Box::new(ResourceChanges(resource.clone())));
    }

    fn send_step(
        clock: Res<SimulationComputeClock>,
        mut buffer: ResMut<Self>,
        sender: Res<StepSender>,
    ) {
        let events = buffer.take();
        if events.is_empty() {
            return;
        }

        sender.send(SimulationStep {
            time: clock.now(),
            events,
        });
    }
}
