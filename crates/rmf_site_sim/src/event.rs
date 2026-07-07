use crate::compute::SimulationComputeClock;
use crate::schedule::{SimulationComputeSet, SimulationComputeStep};
use crate::time::SimulationTime;
use bevy::ecs::system::{Deferred, SystemBuffer, SystemMeta, SystemParam};
use bevy::prelude::*;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::ops::Bound;

/// An event that occurs at a discrete simulation time.
pub struct DiscreteEvent<T: Event> {
    pub time: SimulationTime,
    pub event: T,
}

/// A resource that holds discrete events to be processed at specific simulation times.
#[derive(Resource)]
pub struct DiscreteEvents<T: Event>(BTreeMap<SimulationTime, Vec<DiscreteEvent<T>>>);

impl<T: Event> Default for DiscreteEvents<T> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<T: Event> DiscreteEvents<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedules an event to occur at the given time.
    fn schedule(&mut self, time: SimulationTime, event: T) {
        self.0
            .entry(time)
            .or_default()
            .push(DiscreteEvent { time, event });
    }

    /// The next time after the specified time for which an event exists.
    fn next_time_after(&self, time: SimulationTime) -> Option<SimulationTime> {
        self.0
            .range((Bound::Excluded(time), Bound::Unbounded))
            .next()
            .map(|(t, _)| *t)
    }

    /// Returns the events that occur at a specified time.
    fn events_at(&self, time: SimulationTime) -> Option<&Vec<DiscreteEvent<T>>> {
        self.0.get(&time)
    }

    // TODO: Make efficient
    /// Drains events before the specified time.
    fn drain_events_before(&mut self, time: SimulationTime) {
        self.0.retain(|&t, _| t >= time);
    }

    /// Drains events before the current clock time, and adds the next available event to the clock.
    fn sync_with_clock(
        mut events: ResMut<DiscreteEvents<T>>,
        mut clock: ResMut<SimulationComputeClock>,
    ) {
        events.drain_events_before(clock.now());
        if let Some(time) = events.next_time_after(clock.now()) {
            clock.try_add_pending(time);
        }
    }
}

/// Sends discrete events of type `T`.
#[derive(SystemParam)]
pub struct DiscreteEventWriter<'w, 's, T: Event> {
    buffer: Deferred<'s, DiscreteEventWriteBuffer<T>>,
    clock: Res<'w, SimulationComputeClock>,
}

impl<T: Event> DiscreteEventWriter<'_, '_, T> {
    pub fn schedule(&mut self, time: SimulationTime, event: T) {
        let time_now = self.clock.now();
        if time <= time_now {
            panic!(
                "Tried to schedule an event at time {time:?} \
                that is not later than the current time {time_now:?} \
                and will never be read.",
            );
        }
        self.buffer.0.push(DiscreteEvent { time, event });
    }
}

pub struct DiscreteEventWriteBuffer<T: Event>(Vec<DiscreteEvent<T>>);

impl<T: Event> Default for DiscreteEventWriteBuffer<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T: Event> SystemBuffer for DiscreteEventWriteBuffer<T> {
    fn apply(&mut self, _system_meta: &SystemMeta, world: &mut World) {
        if self.0.is_empty() {
            return;
        }
        let mut events = world.get_resource_mut::<DiscreteEvents<T>>().unwrap();
        for event in self.0.drain(..) {
            events.schedule(event.time, event.event);
        }
    }
}

/// Reads discrete events of type `T`.
#[derive(SystemParam)]
pub struct DiscreteEventReader<'w, T: Event> {
    events: Res<'w, DiscreteEvents<T>>,
    clock: Res<'w, SimulationComputeClock>,
}

impl<T: Event> DiscreteEventReader<'_, T> {
    // TODO: Restrict systems to only allow reading once per step. How does Bevy do it?
    pub fn read(&self) -> impl Iterator<Item = &T> {
        self.events
            .events_at(self.clock.now())
            .into_iter()
            .flat_map(|v| v.iter())
            .map(|event| &event.event)
    }
}

/// Plugin for enabling functionality of discrete events of type `T`.
pub struct DiscreteEventsPlugin<T: Event>(PhantomData<T>);

impl<T: Event> Default for DiscreteEventsPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Event> Clone for DiscreteEventsPlugin<T> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<T: Event> Plugin for DiscreteEventsPlugin<T> {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiscreteEvents<T>>().add_systems(
            SimulationComputeStep,
            DiscreteEvents::<T>::sync_with_clock
                .in_set(SimulationComputeSet::IncrementComputeClock)
                .before(SimulationComputeClock::advance),
        );
    }
}
