use crate::compute::SimulationComputeClock;
use crate::schedule::{SimulationCompute, SimulationComputeStep};
use crate::simulation::{ComponentChanges, SimulationCommand, SimulationStep};
use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use crossbeam_channel::Sender;

#[derive(Clone)]
pub struct EntitySynchronizer {
    synchronizers: Vec<ComponentSynchronizer>,
}

impl EntitySynchronizer {
    pub fn new() -> Self {
        Self {
            synchronizers: Vec::new(),
        }
    }

    /// Registers a component that should be extracted to the simulation world,
    /// but should not be tracked in a [`SimulationStep`] sent to the main world.
    pub fn register_untracked<T: Component + Clone>(&mut self) {
        self.register_component::<T, false>();
    }
    /// Registers a component that should be extracted to the simulation world,
    /// and should be tracked in a [`SimulationStep`] sent to the main world.
    pub fn register_tracked<T: Component + Clone>(&mut self) {
        self.register_component::<T, true>();
    }

    // TODO: What if register_component is called twice for the same type?
    fn register_component<T: Component + Clone, const TRACKED: bool>(&mut self) {
        self.synchronizers
            .push(ComponentSynchronizer::new::<T, TRACKED>());
    }

    /// Extracts all entities that should be synchronized.
    pub fn extract_entities(&self, world: &World) -> Vec<Entity> {
        let mut entities = EntityHashSet::default();
        for synchronizer in &self.synchronizers {
            (synchronizer.extract_entities)(world, &mut entities);
        }
        entities.into_iter().collect()
    }

    /// Extracts all components that should be synchronized.
    pub fn extract_components(&self, world: &World) -> Vec<Box<dyn SimulationCommand>> {
        self.synchronizers
            .iter()
            .map(|synchronizer| (synchronizer.extract_components)(world))
            .collect()
    }
}

impl Plugin for EntitySynchronizer {
    /// Inserts the [`SimulationCommandBuffer`] that tracking systems write to, and adds
    /// the systems for tracking all tracked components along with the flush that sends
    /// each [`SimulationStep`] back to the main world.
    ///
    /// The compute app must also contain a [`StepSender`] resource for the flush to use.
    fn build(&self, app: &mut App) {
        app.init_resource::<SimulationCommandBuffer>();

        for synchronizer in &self.synchronizers {
            if let Some(tracking_system) = synchronizer.tracking_system {
                app.add_systems(
                    SimulationComputeStep,
                    tracking_system().in_set(SimulationCompute::BufferChangedComponents),
                );
            }
        }
        app.add_systems(
            SimulationComputeStep,
            SimulationCommandBuffer::flush.in_set(SimulationCompute::SendSimulationStep),
        );
    }
}

/// A synchronizer for a single component type.
#[derive(Clone, Copy)]
struct ComponentSynchronizer {
    extract_entities: fn(&World, &mut EntityHashSet),
    extract_components: fn(&World) -> Box<dyn SimulationCommand>,
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
                Some(|| SimulationCommandBuffer::buffer_changes::<T>.into_configs())
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

/// A buffer to store a series of [`SimulationCommand`]s built in the current step, to be flushed
/// back to the main world as a single [`SimulationStep`].
#[derive(Resource, Default)]
pub struct SimulationCommandBuffer(Vec<Box<dyn SimulationCommand>>);

impl SimulationCommandBuffer {
    fn push(&mut self, command: Box<dyn SimulationCommand>) {
        self.0.push(command);
    }

    fn take(&mut self) -> Vec<Box<dyn SimulationCommand>> {
        std::mem::take(&mut self.0)
    }

    fn buffer_changes<T: Component + Clone>(
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

    fn flush(
        clock: Res<SimulationComputeClock>,
        mut buffer: ResMut<Self>,
        sender: Res<StepSender>,
    ) {
        let commands = buffer.take();
        if commands.is_empty() {
            return;
        }

        sender.send(SimulationStep {
            time: clock.now(),
            commands,
        });
    }
}
