use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::BTreeMap;

use crate::SimTime;
use crate::compute::{AddComputeClock, ComputeClock, ComputeTimeStep, advance_clock};

pub trait DiscreteEvent: Event {
    fn time(&self) -> SimTime;
    fn event_source(&self) -> Entity;
    fn event_target(&self) -> Entity;
}

#[derive(Resource)]
pub struct DiscreteEvents<E: DiscreteEvent> {
    queue: BTreeMap<SimTime, Vec<E>>,
}

impl<E: DiscreteEvent> Default for DiscreteEvents<E> {
    fn default() -> Self {
        Self {
            queue: BTreeMap::new(),
        }
    }
}

impl<E: DiscreteEvent> DiscreteEvents<E> {
    fn next_time(&self) -> Option<SimTime> {
        self.queue.keys().next().copied()
    }

    pub fn at(&mut self, now: SimTime) -> Vec<E> {
        let mut events = Vec::new();
        while let Some((&time, _)) = self.queue.first_key_value() {
            if time > now {
                break;
            }
            let (_, batch) = self.queue.pop_first().unwrap();
            events.extend(batch);
        }
        events
    }
}

pub fn update_clock<E: DiscreteEvent>(
    events: Res<DiscreteEvents<E>>,
    mut clock: ResMut<ComputeClock>,
) {
    if let Some(time) = events.next_time() {
        clock.add(time);
    }
}

#[derive(SystemParam)]
pub struct DiscreteEventReader<'w, E: DiscreteEvent> {
    events: ResMut<'w, DiscreteEvents<E>>,
    clock: Res<'w, ComputeClock>,
}

impl<E: DiscreteEvent> DiscreteEventReader<'_, E> {
    pub fn read(&mut self) -> Vec<E> {
        let now = self.clock.now();
        self.events.at(now)
    }
}

#[derive(SystemParam)]
pub struct DiscreteEventWriter<'w, E: DiscreteEvent> {
    events: ResMut<'w, DiscreteEvents<E>>,
}

impl<E: DiscreteEvent> DiscreteEventWriter<'_, E> {
    pub fn write(&mut self, event: E) {
        self.events
            .queue
            .entry(event.time())
            .or_default()
            .push(event);
    }
}

pub trait RegisterDiscreteEvent {
    fn register_discrete_event<E: DiscreteEvent>(&mut self) -> &mut Self;
}

impl RegisterDiscreteEvent for App {
    fn register_discrete_event<E: DiscreteEvent>(&mut self) -> &mut Self {
        self.add_compute_clock()
            .init_resource::<DiscreteEvents<E>>()
            .add_systems(ComputeTimeStep, update_clock::<E>.before(advance_clock))
    }
}
