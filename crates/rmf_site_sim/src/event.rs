use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::BTreeMap;

use crate::SimTime;
use crate::compute::{AddComputeClock, ComputeClock, ComputeTimeStep, advance_clock};

pub struct DiscreteEvent<T: Event> {
    pub time: SimTime,
    pub event: T,
}

#[derive(Resource)]
pub struct DiscreteEvents<T: Event> {
    queue: BTreeMap<SimTime, Vec<T>>,
}

impl<T: Event> Default for DiscreteEvents<T> {
    fn default() -> Self {
        Self {
            queue: BTreeMap::new(),
        }
    }
}

impl<T: Event> DiscreteEvents<T> {
    fn next_time(&self) -> Option<SimTime> {
        self.queue.keys().next().copied()
    }

    pub fn at(&mut self, now: SimTime) -> Vec<DiscreteEvent<T>> {
        let mut events = Vec::new();
        while let Some((&time, _)) = self.queue.first_key_value() {
            if time > now {
                break;
            }
            let (time, batch) = self.queue.pop_first().unwrap();
            events.extend(batch.into_iter().map(|event| DiscreteEvent { time, event }));
        }
        events
    }
}

pub fn update_clock<T: Event>(events: Res<DiscreteEvents<T>>, mut clock: ResMut<ComputeClock>) {
    if let Some(time) = events.next_time() {
        clock.add(time);
    }
}

#[derive(SystemParam)]
pub struct DiscreteEventReader<'w, T: Event> {
    events: ResMut<'w, DiscreteEvents<T>>,
    clock: Res<'w, ComputeClock>,
}

impl<T: Event> DiscreteEventReader<'_, T> {
    pub fn read(&mut self) -> Vec<DiscreteEvent<T>> {
        let now = self.clock.now();
        self.events.at(now)
    }
}

#[derive(SystemParam)]
pub struct DiscreteEventWriter<'w, T: Event> {
    events: ResMut<'w, DiscreteEvents<T>>,
}

impl<T: Event> DiscreteEventWriter<'_, T> {
    pub fn write(&mut self, event: DiscreteEvent<T>) {
        self.events
            .queue
            .entry(event.time)
            .or_default()
            .push(event.event);
    }
}

pub trait RegisterDiscreteEvent {
    fn register_discrete_event<T: Event>(&mut self) -> &mut Self;
}

impl RegisterDiscreteEvent for App {
    fn register_discrete_event<T: Event>(&mut self) -> &mut Self {
        self.add_compute_clock()
            .init_resource::<DiscreteEvents<T>>()
            .add_systems(ComputeTimeStep, update_clock::<T>.before(advance_clock))
    }
}
