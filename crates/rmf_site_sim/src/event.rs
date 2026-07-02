use crate::compute::SimulationComputeClock;
use crate::schedule::{SimulationCompute, SimulationComputeStep};
use crate::time::SimulationTime;
use bevy::prelude::*;
use std::marker::PhantomData;

pub struct DiscreteEvent<T: Event> {
    pub time: SimulationTime,
    pub event: T,
}

#[derive(Resource)]
pub struct DiscreteEvents<T: Event> {
    /// Owned events, kept sorted ascending by `time`.
    queue: Vec<DiscreteEvent<T>>,
}

impl<T: Event> Default for DiscreteEvents<T> {
    fn default() -> Self {
        Self { queue: Vec::new() }
    }
}

impl<T: Event> DiscreteEvents<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedules `event` to occur at `time`.
    pub fn schedule(&mut self, time: SimulationTime, event: T) {
        let index = self.queue.partition_point(|queued| queued.time <= time);
        self.queue.insert(index, DiscreteEvent { time, event });
    }

    fn next_time(&self) -> Option<SimulationTime> {
        self.queue.first().map(|event| event.time)
    }

    fn at(&self, now: SimulationTime) -> &[DiscreteEvent<T>] {
        let end = self.queue.partition_point(|event| event.time <= now);
        &self.queue[..end]
    }

    pub fn update_clock(events: Res<DiscreteEvents<T>>, mut clock: ResMut<SimulationComputeClock>) {
        if let Some(time) = events.next_time() {
            clock.add(time);
        }
    }
}

/// Registers [`DiscreteEvents<T>`] in the compute world and keeps the
/// [`SimulationComputeClock`] scheduled with the next pending event time.
pub struct DiscreteEventsPlugin<T: Event>(PhantomData<T>);

impl<T: Event> Default for DiscreteEventsPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Event> DiscreteEventsPlugin<T> {
    fn new() -> Self {
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
            DiscreteEvents::<T>::update_clock.in_set(SimulationCompute::ScheduleNextStep),
        );
    }
}
