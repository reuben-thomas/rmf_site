use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::hash::Hash;

use bevy::prelude::*;

pub trait SimulationTime: Ord + Hash + Copy + Send + Sync + 'static {}

impl<T: Ord + Hash + Copy + Send + Sync + 'static> SimulationTime for T {}

/// A Clock to track the current time, and step through a sequence of time-steps.
#[derive(Resource, Default)]
pub struct SimulationClock<T: Send + Sync + 'static> {
    current: T,
    pending_times_heap: BinaryHeap<Reverse<T>>,
    pending_times_set: HashSet<T>,
}

impl<T: SimulationTime> SimulationClock<T> {
    /// The current time.
    pub fn now(&self) -> T {
        self.current
    }

    /// Add a new time to be processed.
    pub fn push(&mut self, time: T) {
        // TODO: Better error handling here.
        if time <= self.current {
            panic!("Time is not greater than current time.")
        }
        if self.pending_times_set.insert(time) {
            self.pending_times_heap.push(Reverse(time));
        }
    }

    /// Advance the clock to the next time-step, returning the new time-step if one exists.
    pub fn step(&mut self) -> Option<T> {
        let Reverse(time) = self.pending_times_heap.pop()?;
        self.pending_times_set.remove(&time);
        self.current = time;
        Some(time)
    }
}

pub fn advance_clock<T: SimulationTime>(
    mut clock: ResMut<SimulationClock<T>>,
    mut exit: EventWriter<AppExit>,
) {
    // TODO: Trigger an appropriate event instead.
    if clock.step().is_none() {
        exit.write(AppExit::Success);
    }
}

pub trait AddSimulationClock {
    fn init_simulation_clock<T: Default + SimulationTime>(&mut self) -> &mut Self;
}

impl AddSimulationClock for App {
    fn init_simulation_clock<T: Default + SimulationTime>(&mut self) -> &mut Self {
        if self.world().contains_resource::<SimulationClock<T>>() {
            return self;
        }
        self.init_resource::<SimulationClock<T>>()
            .add_systems(First, advance_clock::<T>)
    }
}
