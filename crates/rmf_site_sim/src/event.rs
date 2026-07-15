use crate::compute::SimulationComputeClock;
use crate::sync::SimulationEventBuffer;
use crate::time::SimulationTime;
use bevy::ecs::system::{Command, Deferred, SystemBuffer, SystemMeta, SystemParam};
use bevy::prelude::*;
use bevy::utils::synccell::SyncCell;
use std::cmp::Ordering;

/// A unique change applied on the world at a discrete simulation time.
pub trait DiscreteEvent: Send + 'static {
    /// Applies the event to the world.
    fn apply(self: Box<Self>, world: &mut World);

    /// Clones into a boxed object.
    fn clone_to_box(&self) -> Box<dyn DiscreteEvent>;
}

impl<T: Command + Clone> DiscreteEvent for T {
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
    #[expect(clippy::type_complexity)]
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
    fn next_time(&self) -> Option<SimulationTime> {
        self.scheduled.as_ref().map(|(nearest, _)| *nearest)
    }

    /// Removes and returns the events scheduled at the specified time.
    fn take_events_at(&mut self, time: SimulationTime) -> Vec<Box<dyn DiscreteEvent>> {
        let mut events = std::mem::take(self.instant.get());
        if let Some((scheduled_time, _)) = &self.scheduled
            && *scheduled_time == time {
                let (_, scheduled) = self.scheduled.take().unwrap();
                events.extend(SyncCell::to_inner(scheduled));
            }
        events
    }

    /// Adds the next scheduled event time to the clock.
    pub(crate) fn sync_with_clock(
        events: Res<DiscreteEvents>,
        mut clock: ResMut<SimulationComputeClock>,
    ) {
        if let Some(time) = events.next_time() {
            clock.try_add_pending(time);
        }
    }
}

pub fn execute_events(world: &mut World) {
    let now = world.resource::<SimulationComputeClock>().now();
    let events = world.resource_mut::<DiscreteEvents>().take_events_at(now);
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

/// Schedules [`DiscreteEvent`]s, either in the current time step or in the future.
#[derive(SystemParam)]
pub struct DiscreteChange<'w, 's> {
    buffer: Deferred<'s, DiscreteChangeBuffer>,
    clock: Res<'w, SimulationComputeClock>,
    events: Res<'w, DiscreteEvents>,
}

impl DiscreteChange<'_, '_> {
    pub fn schedule(&mut self, time: SimulationTime, event: impl DiscreteEvent) {
        let time_now = self.clock.now();
        match time.cmp(&time_now) {
            Ordering::Less => panic!(
                "Tried to schedule an event at time {time:?} \
                that is earlier than the current time {time_now:?} \
                and will never be executed.",
            ),
            Ordering::Equal => self.buffer.instant.push(Box::new(event)),
            Ordering::Greater => {
                if !self.may_schedule(time) {
                    return;
                }
                match &mut self.buffer.scheduled {
                    Some((nearest, events)) => match time.cmp(nearest) {
                        Ordering::Less => {
                            *nearest = time;
                            *events = vec![Box::new(event)];
                        }
                        Ordering::Equal => events.push(Box::new(event)),
                        Ordering::Greater => {}
                    },
                    None => {
                        self.buffer.scheduled = Some((time, vec![Box::new(event)]));
                    }
                }
            }
        }
    }

    pub fn schedule_now(&mut self, event: impl DiscreteEvent) {
        self.buffer.instant.push(Box::new(event));
    }

    /// Identify if an event can be scheduled at the given time.
    pub fn may_schedule(&self, time: SimulationTime) -> bool {
        match time.cmp(&self.clock.now()) {
            Ordering::Less => false,
            Ordering::Equal => true,
            Ordering::Greater => {
                if let Some((next_time, _)) = &self.buffer.scheduled
                    && time > *next_time {
                        return false;
                    }
                if let Some(next_time) = self.events.next_time()
                    && time > next_time {
                        return false;
                    }
                true
            }
        }
    }
}

#[derive(Default)]
pub struct DiscreteChangeBuffer {
    instant: Vec<Box<dyn DiscreteEvent>>,
    scheduled: Option<(SimulationTime, Vec<Box<dyn DiscreteEvent>>)>,
}

impl SystemBuffer for DiscreteChangeBuffer {
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
