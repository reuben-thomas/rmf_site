use bevy::ecs::component::ComponentId;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use std::collections::HashSet;

pub use compute::*;
pub use event::*;
pub use time::*;

mod compute;
mod event;
mod time;

pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ComputePlugin)
            .add_event::<ComputeSimulation>()
            .add_systems(Update, queue_simulation_runs);
    }
}

#[derive(Event, Default)]
pub struct ComputeSimulation;

fn queue_simulation_runs(mut events: EventReader<ComputeSimulation>, mut commands: Commands) {
    for _ in events.read() {
        commands.queue(run_simulations);
    }
}

fn run_simulations(world: &mut World) {
    let entities: Vec<Entity> = world
        .query_filtered::<Entity, With<SimulationSet>>()
        .iter(world)
        .collect();

    for entity in entities {
        let Some(mut simulation_set) = world.entity_mut(entity).take::<SimulationSet>() else {
            continue;
        };
        simulation_set.run(world);
        world.entity_mut(entity).insert(simulation_set);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SimulationSystems {
    Clock,
    Compute,
}

/// A set of entities, components, systems, and resources that can be used to compute a simulation.
#[derive(Component)]
pub struct SimulationSet {
    setup_schedule: Schedule,
    compute_schedule: Schedule,
    untracked_components: HashSet<ComponentId>,
    tracked_components: HashSet<ComponentId>,
}

impl SimulationSet {
    fn new() -> Self {
        let mut compute_schedule = Schedule::new(ComputeTimeStep);
        compute_schedule
            .configure_sets(SimulationSystems::Clock.before(SimulationSystems::Compute));
        Self {
            setup_schedule: Schedule::new(SetupSimulation),
            compute_schedule,
            untracked_components: HashSet::default(),
            tracked_components: HashSet::default(),
        }
    }

    fn run(&mut self, world: &mut World) {
        world.insert_resource(ComputeClock::default());
        self.setup_schedule.run(world);
        loop {
            let previous = world.resource::<ComputeClock>().now();
            self.compute_schedule.run(world);
            if world.resource::<ComputeClock>().now() == previous {
                break;
            }
        }
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SetupSimulation;

pub struct SimulationSetBuilder<'a> {
    world: &'a mut World,
    simulation_set: SimulationSet,
}

impl<'a> SimulationSetBuilder<'a> {
    pub fn new(world: &'a mut World) -> Self {
        Self {
            world,
            simulation_set: SimulationSet::new(),
        }
    }

    pub fn add_setup_systems<M>(
        mut self,
        system: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> Self {
        self.simulation_set.setup_schedule.add_systems(system);
        self
    }

    // TODO:
    // - Add a deterministic total ordering for systems unless otherwise specified in the schedule configs
    pub fn add_systems<M>(mut self, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) -> Self {
        self.simulation_set
            .compute_schedule
            .add_systems(systems.in_set(SimulationSystems::Compute));
        self
    }

    pub fn register_tracked_component<C: Component>(mut self) -> Self {
        let id = self.world.register_component::<C>();
        self.simulation_set.untracked_components.remove(&id);
        self.simulation_set.tracked_components.insert(id);
        self
    }

    // TODO:
    // - Can events be automatically registered from the provided systems?
    pub fn register_event<E: DiscreteEvent>(mut self) -> Self {
        if !self.world.contains_resource::<ComputeClock>() {
            self.world.init_resource::<ComputeClock>();
            self.simulation_set.compute_schedule.add_systems(
                advance_clock
                    .in_set(SimulationSystems::Clock)
                    .run_if(in_state(ComputeState::Computing)),
            );
        }

        self.world.init_resource::<DiscreteEvents<E>>();
        self.simulation_set.compute_schedule.add_systems(
            update_clock::<E>
                .in_set(SimulationSystems::Clock)
                .before(advance_clock),
        );
        self
    }

    fn register_system_components_as_untracked(&mut self) {
        if self
            .simulation_set
            .compute_schedule
            .initialize(self.world)
            .is_err()
        {
            panic!("Unable to initialize schedule with world");
        }
        let Ok(systems) = self.simulation_set.compute_schedule.systems() else {
            panic!("Unable to retrieve systems from schedule");
        };

        let accessed: Vec<ComponentId> = systems
            .filter_map(|(_, system)| system.component_access().try_iter_component_access().ok())
            .flatten()
            .map(|access| *access.index())
            .collect();

        for id in accessed {
            if !self.simulation_set.tracked_components.contains(&id) {
                self.simulation_set.untracked_components.insert(id);
            }
        }
    }

    pub fn build(mut self) -> SimulationSet {
        self.register_system_components_as_untracked();
        self.simulation_set
    }
}
