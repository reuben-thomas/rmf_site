use crate::compute::SimulationComputeClock;
use crate::sync::SimulationEventBuffer;
use crate::time::SimulationTime;
use bevy::ecs::system::{Command, Deferred, SystemBuffer, SystemMeta, SystemParam};
use bevy::prelude::*;
use bevy::utils::synccell::SyncCell;
use std::cmp::Ordering;
use std::marker::PhantomData;

/// A unique change applied on the world at a discrete simulation time.
pub trait DiscreteEvent: Send + 'static {
    /// Applies the event to the world.
    fn apply(self: Box<Self>, world: &mut World);

    /// Clones into a boxed object.
    fn clone_to_box(&self) -> Box<dyn DiscreteEvent>;
}

impl<T> DiscreteEvent for T
where
    T: Command + Clone,
{
    fn apply(self: Box<Self>, world: &mut World) {
        Command::apply(*self, world)
    }

    fn clone_to_box(&self) -> Box<dyn DiscreteEvent> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn DiscreteEvent> {
    fn clone(&self) -> Self {
        self.clone_to_box()
    }
}

/// A resource holding discrete events to be executed at specific simulation times.
#[derive(Resource)]
pub struct DiscreteEvents {
    instant: SyncCell<Vec<Box<dyn DiscreteEvent>>>,
    scheduled: Option<(SimulationTime, SyncCell<Vec<Box<dyn DiscreteEvent>>>)>,
}

impl Default for DiscreteEvents {
    fn default() -> Self {
        Self {
            instant: SyncCell::new(Vec::new()),
            scheduled: None,
        }
    }
}

impl DiscreteEvents {
    fn try_schedule(
        &mut self,
        now: SimulationTime,
        event_time: SimulationTime,
        event: Box<dyn DiscreteEvent>,
    ) {
        match event_time.cmp(&now) {
            Ordering::Less => {}
            Ordering::Equal => self.instant.get().push(event),
            Ordering::Greater => match &mut self.scheduled {
                Some((nearest, events)) => match event_time.cmp(nearest) {
                    Ordering::Less => {
                        *nearest = event_time;
                        *events = SyncCell::new(vec![event]);
                    }
                    Ordering::Equal => events.get().push(event),
                    Ordering::Greater => {}
                },
                None => {
                    self.scheduled = Some((event_time, SyncCell::new(vec![event])));
                }
            },
        }
    }

    /// The nearest scheduled event time, which is always in the future.
    pub fn next_time(&self) -> Option<SimulationTime> {
        self.scheduled.as_ref().map(|(nearest, _)| *nearest)
    }

    /// Removes and returns the events scheduled at the specified time.
    fn take_scheduled_at(&mut self, time: SimulationTime) -> Vec<Box<dyn DiscreteEvent>> {
        match &self.scheduled {
            Some((scheduled_time, _)) if *scheduled_time == time => {
                let (_, scheduled) = self.scheduled.take().unwrap();
                SyncCell::to_inner(scheduled)
            }
            _ => Vec::new(),
        }
    }

    /// Removes and returns the first instant event, discarding the others.
    fn take_first_instant(&mut self) -> Option<Box<dyn DiscreteEvent>> {
        std::mem::take(self.instant.get()).into_iter().next()
    }
}

/// Executes all events scheduled at the current simulation time.
pub fn execute_scheduled_events(world: &mut World) {
    let now = world.resource::<SimulationComputeClock>().now();
    let events = world
        .resource_mut::<DiscreteEvents>()
        .take_scheduled_at(now);
    if events.is_empty() {
        return;
    }

    world
        .resource_mut::<SimulationEventBuffer>()
        .extend(events.iter().cloned());
    for event in events {
        event.apply(world);
    }
}

pub fn execute_first_instant_event(world: &mut World) -> bool {
    let Some(event) = world.resource_mut::<DiscreteEvents>().take_first_instant() else {
        return false;
    };

    world
        .resource_mut::<SimulationEventBuffer>()
        .extend([event.clone()]);
    event.apply(world);
    true
}

#[derive(SystemParam)]
pub struct DiscreteChangeWriter<'w, 's> {
    buffer: Deferred<'s, DiscreteEventBuffer>,
    clock: Res<'w, SimulationComputeClock>,
}

impl DiscreteChangeWriter<'_, '_> {
    pub fn write(&mut self, time: SimulationTime, event: impl DiscreteEvent) {
        self.buffer.queue(self.clock.now(), time, Box::new(event));
    }

    pub fn write_now(&mut self, event: impl DiscreteEvent) {
        self.buffer.queue_now(Box::new(event));
    }
}

#[derive(SystemParam)]
pub struct DiscreteComponentWriter<'w, 's, T: Component + Clone> {
    buffer: Deferred<'s, DiscreteEventBuffer>,
    clock: Res<'w, SimulationComputeClock>,
    _marker: PhantomData<T>,
}

impl<T: Component + Clone> DiscreteComponentWriter<'_, '_, T> {
    pub fn write(&mut self, time: SimulationTime, entity: Entity, value: T) {
        self.buffer.queue(
            self.clock.now(),
            time,
            Box::new(DiscreteComponentWrite { entity, value }),
        );
    }

    pub fn write_now(&mut self, entity: Entity, value: T) {
        self.buffer
            .queue_now(Box::new(DiscreteComponentWrite { entity, value }));
    }
}

#[derive(Clone)]
struct DiscreteComponentWrite<T: Component + Clone> {
    entity: Entity,
    value: T,
}

// TODO: Handle failures
impl<T: Component + Clone> Command for DiscreteComponentWrite<T> {
    fn apply(self, world: &mut World) {
        world.entity_mut(self.entity).insert(self.value);
    }
}

#[derive(SystemParam)]
pub struct DiscreteResourceWriter<'w, 's, R: Resource + Clone> {
    buffer: Deferred<'s, DiscreteEventBuffer>,
    clock: Res<'w, SimulationComputeClock>,
    _marker: PhantomData<R>,
}

impl<R: Resource + Clone> DiscreteResourceWriter<'_, '_, R> {
    pub fn write(&mut self, time: SimulationTime, value: R) {
        self.buffer.queue(
            self.clock.now(),
            time,
            Box::new(DiscreteResourceWrite { value }),
        );
    }

    pub fn write_now(&mut self, value: R) {
        self.buffer
            .queue_now(Box::new(DiscreteResourceWrite { value }));
    }
}

#[derive(Clone)]
struct DiscreteResourceWrite<R: Resource + Clone> {
    value: R,
}

// TODO: Fail if the resource already exists?
impl<R: Resource + Clone> Command for DiscreteResourceWrite<R> {
    fn apply(self, world: &mut World) {
        world.insert_resource(self.value);
    }
}

#[derive(Default)]
pub struct DiscreteEventBuffer {
    instant: Vec<Box<dyn DiscreteEvent>>,
    scheduled: Option<(SimulationTime, Vec<Box<dyn DiscreteEvent>>)>,
}

impl DiscreteEventBuffer {
    fn queue(&mut self, now: SimulationTime, time: SimulationTime, event: Box<dyn DiscreteEvent>) {
        match time.cmp(&now) {
            Ordering::Less => panic!(
                "Tried to schedule an event at time {time:?} \
                that is earlier than the current time {now:?} \
                and will never be executed.",
            ),
            Ordering::Equal => self.instant.push(event),
            Ordering::Greater => match &mut self.scheduled {
                Some((nearest, events)) => match time.cmp(nearest) {
                    Ordering::Less => {
                        *nearest = time;
                        *events = vec![event];
                    }
                    Ordering::Equal => events.push(event),
                    Ordering::Greater => {}
                },
                None => {
                    self.scheduled = Some((time, vec![event]));
                }
            },
        }
    }

    fn queue_now(&mut self, event: Box<dyn DiscreteEvent>) {
        self.instant.push(event);
    }
}

impl SystemBuffer for DiscreteEventBuffer {
    fn apply(&mut self, _system_meta: &SystemMeta, world: &mut World) {
        if self.instant.is_empty() && self.scheduled.is_none() {
            return;
        }
        let now = world.resource::<SimulationComputeClock>().now();
        let mut events = world.get_resource_mut::<DiscreteEvents>().unwrap();
        for event in self.instant.drain(..) {
            events.try_schedule(now, now, event);
        }
        if let Some((time, scheduled)) = self.scheduled.take() {
            for event in scheduled {
                events.try_schedule(now, time, event);
            }
        }
    }
}
