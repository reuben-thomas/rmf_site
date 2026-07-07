use crate::compute::SimulationComputeClock;
use crate::schedule::SimulationComputeSet;
use crate::simulation::{ComponentChanges, SimulationCommand, SimulationStep};
use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use crossbeam_channel::Sender;

#[derive(Default, Clone)]
pub struct EntitySynchronizer {
    synchronizers: Vec<ComponentSynchronizer>,
}

impl EntitySynchronizer {
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

    /// Inserts the resources required for tracking changes to components.
    pub fn configure_tracking(
        &self,
        world: &mut World,
        schedule: &mut Schedule,
        step_sender: Sender<SimulationStep>,
    ) {
        world.init_resource::<SimulationCommandBuffer>();
        world.insert_resource(StepSender::new(step_sender));

        for synchronizer in &self.synchronizers {
            if let Some(tracking_system) = synchronizer.tracking_system {
                schedule.add_systems(
                    tracking_system()
                        .before(SimulationCommandBuffer::send_step)
                        .in_set(SimulationComputeSet::SendSimulationStep),
                );
            }
        }
        schedule.add_systems(
            SimulationCommandBuffer::send_step
                .before(SimulationComputeClock::advance)
                .in_set(SimulationComputeSet::SendSimulationStep),
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

/// A buffer to store a series of [`SimulationCommand`]s built in the current step,
/// sent to the main world as a single [`SimulationStep`].
#[derive(Resource, Default)]
pub struct SimulationCommandBuffer(Vec<Box<dyn SimulationCommand>>);

impl SimulationCommandBuffer {
    fn push(&mut self, command: Box<dyn SimulationCommand>) {
        self.0.push(command);
    }

    fn take(&mut self) -> Vec<Box<dyn SimulationCommand>> {
        std::mem::take(&mut self.0)
    }

    // TODO:
    // - verify change with partialeq
    // - store a full snapshot every x steps
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

    fn send_step(
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
