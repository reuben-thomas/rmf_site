use crate::schedule::{
    ScheduleBuilder, SimulationComputeSet, SimulationComputeStep, SimulationStartup,
};
use crate::simulation::{PluginFactory, SimulationInitStep, SimulationStep};
use crate::sync::EntitySynchronizer;
use crate::time::SimulationTime;
use bevy::prelude::*;
use crossbeam_channel::Sender;
use std::collections::BTreeSet;

// TODO: Error handling, this is being executed in a separate thread.
pub fn compute_simulation(compute_plugin: SimulationComputePlugin, plugins: Vec<PluginFactory>) {
    let mut app = App::new();
    app.add_plugins(compute_plugin);
    for plugin in &plugins {
        plugin(&mut app);
    }
    app.run();
}

pub struct SimulationComputePlugin {
    pub synchronizer: EntitySynchronizer,
    pub startup_schedule_builder: ScheduleBuilder,
    pub compute_schedule_builder: ScheduleBuilder,
    pub entities: Vec<Entity>,
    pub init_step: SimulationInitStep,
    pub step_sender: Sender<SimulationStep>,
}

impl Plugin for SimulationComputePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimulationComputeClock>();

        let world = app.world_mut();
        for entity in &self.entities {
            spawn_at(world, *entity);
        }
        for command in self.init_step.commands.clone() {
            command.apply(world);
        }

        let startup_schedule = self.startup_schedule_builder.build();
        let mut system_schedule = self.compute_schedule_builder.build();
        // TODO: Should the following configuration logic be within the builder?
        system_schedule.configure_sets(
            (
                SimulationComputeSet::ExecuteSystems,
                SimulationComputeSet::SendSimulationStep,
                SimulationComputeSet::IncrementComputeClock,
            )
                .chain(),
        );
        system_schedule.add_systems(
            SimulationComputeClock::advance.in_set(SimulationComputeSet::IncrementComputeClock),
        );
        self.synchronizer.configure_tracking(
            app.world_mut(),
            &mut system_schedule,
            self.step_sender.clone(),
        );

        app.add_schedule(startup_schedule);
        app.add_schedule(system_schedule);
        app.set_runner(run_simulation);
    }
}

fn run_simulation(mut app: App) -> AppExit {
    app.world_mut().run_schedule(SimulationStartup);
    loop {
        app.world_mut().run_schedule(SimulationComputeStep);

        // TODO: This should be a generic trait with a builtin impl,
        // e.g. crate::simulation::EndCondition
        if app.world().resource::<SimulationComputeClock>().at_end() {
            break;
        }
    }
    AppExit::Success
}

#[derive(Resource, Default)]
pub struct SimulationComputeClock {
    current: SimulationTime,
    pending: BTreeSet<SimulationTime>,
}

impl SimulationComputeClock {
    pub fn now(&self) -> SimulationTime {
        self.current
    }

    pub fn add(&mut self, time: SimulationTime) {
        // TODO: Better error handling than panicking
        if time <= self.current {
            panic!(
                "Tried to add time {time:?} that is not greater than the current time {:?}.",
                self.now()
            )
        }
        self.pending.insert(time);
    }

    fn at_end(&self) -> bool {
        self.pending.is_empty()
    }

    fn next(&mut self) -> Option<SimulationTime> {
        let time = self.pending.pop_first()?;
        self.current = time;
        Some(time)
    }

    pub fn advance(mut clock: ResMut<SimulationComputeClock>) {
        if clock.at_end() {
            debug!("Compute clock reached end at time {:?}", clock.now());
            return;
        }

        clock.next();
    }
}

// TODO: This method is only in newer versions of Bevy, this is a workaround.
// https://docs.rs/bevy/latest/bevy/prelude/struct.World.html#method.spawn_at
fn spawn_at(world: &mut World, entity: Entity) {
    #[allow(deprecated)]
    let _ = world.insert_or_spawn_batch([(entity, ())]);
}
