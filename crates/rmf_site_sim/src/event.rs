use crate::compute::SimulationComputeClock;
use crate::time::SimulationTime;
use bevy::ecs::system::{Deferred, SystemBuffer, SystemMeta, SystemParam};
use bevy::prelude::*;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::Bound;

// TODO:
// - Should time be contained within an event?
// - impl To, From bevy::Command
/// A discrete event represents a unique change to the [`World`] that is scheduled to be
/// applied at a discrete simulation time.
pub trait DiscreteEvent: Send + Sync + 'static {
    fn apply(self: Box<Self>, world: &mut World);

    fn clone_to_box(&self) -> Box<dyn DiscreteEvent>;
}

impl Clone for Box<dyn DiscreteEvent> {
    fn clone(&self) -> Self {
        self.clone_to_box()
    }
}

impl<F> DiscreteEvent for F
where
    F: FnOnce(&mut World) + Clone + Send + Sync + 'static,
{
    fn apply(self: Box<Self>, world: &mut World) {
        (*self)(world)
    }

    fn clone_to_box(&self) -> Box<dyn DiscreteEvent> {
        Box::new(self.clone())
    }
}

/// A resource that holds discrete events for each simulation time.
#[derive(Default, Resource)]
pub struct DiscreteEvents(BTreeMap<SimulationTime, Vec<Box<dyn DiscreteEvent>>>);

impl DiscreteEvents {
    /// Add a a discrete event at the specified simulation time.
    fn add(&mut self, time: SimulationTime, event: Box<dyn DiscreteEvent>) {
        self.0.entry(time).or_default().push(event);
    }

    /// System that applies the events scheduled for the current time onto the world.
    pub fn apply_current(world: &mut World) {
        let now = world.resource::<SimulationComputeClock>().now();
        // let events = world.resource_mut::<DiscreteEvents>().take_at 

        // for event in events {
        //     event.apply(world);
        // }
    }
}

impl DiscreteEvents {
    fn next_time_after(&self, time: SimulationTime) -> Option<SimulationTime> {
        self.0
            .range((Bound::Excluded(time), Bound::Unbounded))
            .next()
            .map(|(t, _)| *t)
    }

    /// System that adds the next scheduled event time to the clock.
    pub fn sync_with_clock(
        events: Res<DiscreteEvents>,
        mut clock: ResMut<SimulationComputeClock>,
    ) {
        if let Some(time) = events.next_time_after(clock.now()) {
            clock.insert_pending(time);
        }
    }
}

// TODO:
// - Retrun result to indicate success
// - Lazy API to check if worth computing a change for a timestamp if guaranteed to be discarded.
// - Provide a more convenient API than submitting closures.
/// Submit changes to the world in the current time step or later.
#[derive(SystemParam)]
pub struct DiscreteChange<'w, 's> {
    events: Deferred<'s, DiscreteEventBuffer>,
    clock: Res<'w, SimulationComputeClock>,
}

impl DiscreteChange<'_, '_> {
    pub fn schedule<E: DiscreteEvent>(&mut self, time: SimulationTime, event: E) {
        let time_now = self.clock.now();
        match time.cmp(&time_now) {
            Ordering::Less => {
                panic!(
                    "Tried to schedule a change at time {time:?} \
                that is earlier than the current time {time_now:?} \
                and will never be applied.",
                );
            }
            Ordering::Greater | Ordering::Equal => self.events.try_insert(time, event),
        }
    }
}

/// A buffer to store only the earlieest set of discrete events.
#[derive(Default)]
pub struct DiscreteEventBuffer {
    time: SimulationTime,
    events: Vec<Box<dyn DiscreteEvent>>,
}

impl DiscreteEventBuffer {
    pub fn try_insert<E: DiscreteEvent>(&mut self, time: SimulationTime, event: E) {
        info!("Scheduling event at time {:?}", time);
        match time.cmp(&self.time) {
            Ordering::Greater => {}
            Ordering::Equal => {
                self.events.push(Box::new(event));
            }
            Ordering::Less => {
                self.time = time;
                self.events.clear();
                self.events.push(Box::new(event));
            }
        }
    }
}

impl SystemBuffer for DiscreteEventBuffer {
    fn apply(&mut self, _system_meta: &SystemMeta, world: &mut World) {
        if self.events.is_empty() {
            return;
        }

        let mut events = world.resource_mut::<DiscreteEvents>();
        for event in self.events.drain(..) {
            events.add(self.time, event);
        }
    }
}
