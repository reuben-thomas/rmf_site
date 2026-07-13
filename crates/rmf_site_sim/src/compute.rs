use crate::event::DiscreteEvents;
use crate::schedule::{
    ScheduleBuilder, SimulationComputeSet, SimulationComputeStep, SimulationStartup,
};
use crate::simulation::{PluginFactory, SimulationInitStep, SimulationStep};
use crate::sync::Synchronizer;
use crate::time::SimulationTime;
use bevy::prelude::*;
use crossbeam_channel::Sender;
use std::collections::BTreeSet;

// TODO: Error handling, this is being executed in a separate thread.
/// Computes [`SimulationStep`]s for a simulation.
pub fn compute_simulation(compute_plugin: SimulationComputePlugin, plugins: Vec<PluginFactory>) {
    let mut app = App::new();
    app.add_plugins(compute_plugin);
    for plugin in &plugins {
        plugin(&mut app);
    }
    app.run();
}

/// Plugin that computes [`SimulationStep`]s for a simulation.
pub struct SimulationComputePlugin {
    pub synchronizer: Synchronizer,
    pub startup_schedule_builder: ScheduleBuilder,
    pub compute_schedule_builder: ScheduleBuilder,
    pub entities: Vec<Entity>,
    pub init_step: SimulationInitStep,
    pub step_sender: Sender<SimulationStep>,
}

impl Plugin for SimulationComputePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimulationComputeClock>();
        app.init_resource::<DiscreteEvents>();
        app.init_resource::<DiscreteEvents>();

        let world = app.world_mut();
        for entity in &self.entities {
            spawn_at(world, *entity);
        }
        for event in self.init_step.events.clone() {
            event.apply(world);
        }

        let startup_schedule = self.startup_schedule_builder.build();
        let mut system_schedule = self.compute_schedule_builder.build();
        // TODO: Should the following configuration logic be within the builder?
        system_schedule.configure_sets(
            (
                SimulationComputeSet::ApplyScheduledEvents,
                SimulationComputeSet::ExecuteSystems,
                SimulationComputeSet::SendSimulationStep,
                SimulationComputeSet::IncrementComputeClock,
            )
                .chain(),
        );
        system_schedule.add_systems((
            DiscreteEvents::apply_current.in_set(SimulationComputeSet::ApplyScheduledEvents),
            DiscreteEvents::sync_with_clock
                .in_set(SimulationComputeSet::IncrementComputeClock)
                .before(SimulationComputeClock::advance),
            SimulationComputeClock::advance.in_set(SimulationComputeSet::IncrementComputeClock),
        ));
        self.synchronizer.configure_tracking(
            app.world_mut(),
            &mut system_schedule,
            self.step_sender.clone(),
        );

        app.add_schedule(startup_schedule);
        app.add_schedule(system_schedule);
        app.set_runner(run_compute_simulation);
    }
}

fn run_compute_simulation(mut app: App) -> AppExit {
    app.world_mut().run_schedule(SimulationStartup);
    loop {
        app.world_mut().run_schedule(SimulationComputeStep);

        // TODO: This should be a generic trait with a builtin impl,
        // e.g. crate::simulation::EndCondition
        if app
            .world()
            .resource::<SimulationComputeClock>()
            .is_complete()
        {
            break;
        }
    }
    AppExit::Success
}

/// Clock for tracking the current simulation time being computed, as well as pending times to be processed.
#[derive(Resource, Default)]
pub struct SimulationComputeClock {
    current: SimulationTime,
    pending: BTreeSet<SimulationTime>,
    is_complete: bool,
}

impl SimulationComputeClock {
    /// The current simulation time.
    pub fn now(&self) -> SimulationTime {
        self.current
    }

    /// Insert a pending time to be processed.
    ///
    /// Returns whether a new pending time was added.
    pub fn insert_pending(&mut self, time: SimulationTime) -> bool {
        if time <= self.current {
            panic!(
                "Tried to add time {time:?} that is not greater than the current time {:?}.",
                self.now()
            )
        }
        self.pending.insert(time)
    }

    /// Whether all pending times have been processed.
    fn is_complete(&self) -> bool {
        self.is_complete
    }

    /// Ticks the clock to the next pending time.
    fn tick(&mut self) {
        let time = self
            .pending
            .pop_first()
            .expect("No pending times to increment");
        info!("Compute clock ticked to time {:?}", time);
        self.current = time;
    }

    /// System to advance the compute clock to the next pending time, if one exists.
    pub fn advance(mut clock: ResMut<SimulationComputeClock>) {
        if clock.pending.is_empty() {
            info!("Compute clock reached end at time {:?}", clock.now());
            clock.is_complete = true;
            return;
        }

        clock.tick();
    }
}

// TODO: This method is only in newer versions of Bevy, this is a workaround.
// https://docs.rs/bevy/latest/bevy/prelude/struct.World.html#method.spawn_at
fn spawn_at(world: &mut World, entity: Entity) {
    #[allow(deprecated)]
    let _ = world.insert_or_spawn_batch([(entity, ())]);
}
