use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

use bevy::app::MainScheduleOrder;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use crate::{SimComponent, SimTime};

/// Plugin to allow computing a simulation.
pub struct ComputePlugin;

impl Plugin for ComputePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<StatesPlugin>() {
            app.add_plugins(StatesPlugin);
        }
        app.init_state::<ComputeState>();
        app.world_mut()
            .resource_mut::<MainScheduleOrder>()
            .insert_after(Update, ComputeTimeStep);
    }
}

/// A schedule label for systems that perform computation for a single time step.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ComputeTimeStep;

/// A marker struct indicating the result of the computed simulation at a given time.
#[derive(Component)]
pub struct ComputeResultMarker<T: SimTime> {
    pub time: T,
}

pub struct ComputeResult<T: SimComponent> {
    pub changes: HashMap<Entity, T>,
}

/// The state of computing the simulation.
#[derive(States, Default, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ComputeState {
    #[default]
    Computing,
    Complete,
}

/// A Clock to step through a series of discrete time steps when computing the simulation.
#[derive(Resource, Default)]
pub struct ComputeClock<T: Send + Sync + 'static> {
    current: T,
    pending_times_heap: BinaryHeap<Reverse<T>>,
    pending_times_set: HashSet<T>,
}

impl<T: SimTime> ComputeClock<T> {
    /// The current time.
    pub fn now(&self) -> T {
        self.current
    }

    /// Add a time that should be processed.
    pub fn add(&mut self, time: T) {
        if time <= self.current {
            panic!(
                "Tried to push time {time:?} that is not greater than the current time {:?}.",
                self.now()
            )
        }
        if self.pending_times_set.insert(time) {
            self.pending_times_heap.push(Reverse(time));
        }
    }

    /// Checks if the clock has no remaining pending time steps.
    pub fn at_end(&self) -> bool {
        self.pending_times_heap.is_empty()
    }

    /// Advance the clock to the next time step, and return the new time step if one exists.
    pub fn next(&mut self) -> Option<T> {
        if self.at_end() {
            return None;
        }

        let Reverse(time) = self.pending_times_heap.pop()?;
        self.pending_times_set.remove(&time);
        self.current = time;
        Some(time)
    }
}

pub fn advance_clock<T: SimTime>(
    mut commands: Commands,
    mut clock: ResMut<ComputeClock<T>>,
    mut next_state: ResMut<NextState<ComputeState>>,
) {
    if clock.at_end() {
        next_state.set(ComputeState::Complete);
        info!("Compute clock reached end at time {:?}", clock.now());
        return;
    }

    commands.spawn(ComputeResultMarker { time: clock.now() });
    clock.next();
}

pub trait AddComputeClock {
    fn add_compute_clock<T: Default + SimTime>(&mut self) -> &mut Self;
}

impl AddComputeClock for App {
    fn add_compute_clock<T: Default + SimTime>(&mut self) -> &mut Self {
        if self.world().contains_resource::<ComputeClock<T>>() {
            return self;
        }

        self.init_resource::<ComputeClock<T>>().add_systems(
            ComputeTimeStep,
            advance_clock::<T>.run_if(in_state(ComputeState::Computing)),
        )
    }
}

/// Stores the [`ComputeResult`] for each computed time step.
#[derive(Resource)]
pub struct ComputeResults<Time: SimTime, T: SimComponent> {
    pub results: HashMap<Time, ComputeResult<T>>,
}

impl<Time: SimTime, T: SimComponent> Default for ComputeResults<Time, T> {
    fn default() -> Self {
        Self {
            results: HashMap::new(),
        }
    }
}

pub fn update_compute_result<T: SimTime, C: SimComponent + Clone>(
    clock: Res<ComputeClock<T>>,
    changed: Query<(Entity, &C), Changed<C>>,
    mut results: ResMut<ComputeResults<T, C>>,
) {
    let now = clock.now();
    let result = results.results.entry(now).or_insert_with(|| ComputeResult {
        changes: HashMap::new(),
    });
    for (entity, component) in changed.iter() {
        result.changes.insert(entity, component.clone());
    }
}
