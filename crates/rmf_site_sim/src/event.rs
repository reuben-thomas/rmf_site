use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::BTreeMap;

use crate::SimTime;
use crate::compute::{AddComputeClock, ComputeClock, ComputeTimeStep, advance_clock};

pub trait DiscreteEvent: Event {
    type Time: SimTime;

    fn time(&self) -> Self::Time;
    fn event_source(&self) -> Entity;
    fn event_target(&self) -> Entity;
}

#[derive(Resource)]
pub struct DiscreteEvents<E: DiscreteEvent> {
    queue: BTreeMap<E::Time, Vec<E>>,
}

impl<E: DiscreteEvent> Default for DiscreteEvents<E> {
    fn default() -> Self {
        Self {
            queue: BTreeMap::new(),
        }
    }
}

impl<E: DiscreteEvent> DiscreteEvents<E> {
    pub fn next_time(&self) -> Option<E::Time> {
        self.queue.keys().next().copied()
    }
}

pub fn update_clock<E: DiscreteEvent>(
    events: Res<DiscreteEvents<E>>,
    mut clock: ResMut<ComputeClock<E::Time>>,
) {
    if let Some(time) = events.next_time() {
        clock.add(time);
    }
}

#[derive(SystemParam)]
pub struct DiscreteEventReader<'w, E: DiscreteEvent> {
    events: ResMut<'w, DiscreteEvents<E>>,
    clock: Res<'w, ComputeClock<<E as DiscreteEvent>::Time>>,
}

impl<E: DiscreteEvent> DiscreteEventReader<'_, E> {
    pub fn read(&mut self) -> Vec<E> {
        let now = self.clock.now();
        let ready_times: Vec<E::Time> = self
            .events
            .queue
            .range(..=now)
            .map(|(&time, _)| time)
            .collect();
        let mut events = Vec::new();
        for time in ready_times {
            events.extend(self.events.queue.remove(&time).unwrap());
        }
        events
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
    fn register_discrete_event<E: DiscreteEvent>(&mut self) -> &mut Self
    where
        E::Time: Default;
}

impl RegisterDiscreteEvent for App {
    fn register_discrete_event<E: DiscreteEvent>(&mut self) -> &mut Self
    where
        E::Time: Default,
    {
        self.add_compute_clock::<E::Time>()
            .init_resource::<DiscreteEvents<E>>()
            .add_systems(
                ComputeTimeStep,
                update_clock::<E>.before(advance_clock::<E::Time>),
            )
    }
}
