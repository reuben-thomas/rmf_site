/*
 * Copyright (C) 2023 Open Source Robotics Foundation
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
*/

use crate::{
    inspector::InspectPoseComponent,
    interaction::Selection,
    site::{
        location, update_model_instances, Affiliation, AssetSource, Change, ChangePlugin, Delete,
        DifferentialDrive, Group, LocationTags, MobileRobotMarker, ModelMarker, ModelProperty,
        NameInSite, Pose, Scale, SiteParent, Task, Tasks,
    },
    widgets::{prelude::*, Inspect, InspectionPlugin},
    Icons, ModelPropertyData,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_egui::egui::{Align, Button, Color32, ComboBox, DragValue, Frame, Layout, Stroke, Ui};
use std::any::TypeId;

use super::InspectPose;

#[derive(Default)]
pub struct InspectTaskPlugin {}

impl Plugin for InspectTaskPlugin {
    fn build(&self, app: &mut App) {
        // Allows us to toggle MobileRobotMarker as a configurable property
        // from the model description inspector
        app.register_type::<ModelProperty<MobileRobotMarker>>()
            .add_plugins(ChangePlugin::<ModelProperty<MobileRobotMarker>>::default())
            .add_systems(
                PreUpdate,
                (
                    add_remove_mobile_robot_tasks,
                    update_model_instances::<MobileRobotMarker>,
                ),
            )
            .init_resource::<ModelPropertyData>()
            .world
            .resource_mut::<ModelPropertyData>()
            .optional
            .insert(
                TypeId::of::<ModelProperty<MobileRobotMarker>>(),
                (
                    "Mobile Robot".to_string(),
                    |mut e_cmd| {
                        e_cmd.insert(ModelProperty::<MobileRobotMarker>::default());
                    },
                    |mut e_cmd| {
                        e_cmd.remove::<ModelProperty<MobileRobotMarker>>();
                    },
                ),
            );

        // Ui
        app.init_resource::<PendingTask>().add_plugins((
            ChangePlugin::<Tasks<Entity>>::default(),
            InspectionPlugin::<InspectTasks>::new(),
        ));
    }
}

#[derive(SystemParam)]
pub struct InspectTasks<'w, 's> {
    commands: Commands<'w, 's>,
    selection: Res<'w, Selection>,
    change_tasks: EventWriter<'w, Change<Tasks<Entity>>>,
    mobile_robots:
        Query<'w, 's, &'static mut Tasks<Entity>, (With<MobileRobotMarker>, Without<Group>)>,
    locations: Query<'w, 's, (Entity, &'static NameInSite, &'static LocationTags)>,
    pending_task: ResMut<'w, PendingTask>,
    icons: Res<'w, Icons>,
}

impl<'w, 's> WidgetSystem<Inspect> for InspectTasks<'w, 's> {
    fn show(
        Inspect { selection, .. }: Inspect,
        ui: &mut Ui,
        state: &mut SystemState<Self>,
        world: &mut World,
    ) {
        let mut params = state.get_mut(world);
        let Ok(mut tasks) = params.mobile_robots.get_mut(selection) else {
            return;
        };

        ui.label("Tasks");
        Frame::default()
            .inner_margin(4.0)
            .rounding(2.0)
            .stroke(Stroke::new(1.0, Color32::GRAY))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                if tasks.0.is_empty() {
                    ui.label("No Tasks");
                } else {
                    let mut deleted_ids = Vec::new();
                    for (id, task) in tasks.0.iter_mut().enumerate() {
                        let is_deleted =
                            edit_task_component(ui, &id, task, &params.locations, true);
                        if is_deleted {
                            deleted_ids.push(id);
                        }
                    }
                    for id in deleted_ids {
                        tasks.0.remove(id);
                    }
                }
            });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            // Only allow adding as task if valid
            ui.add_enabled_ui(params.pending_task.0.is_valid(), |ui| {
                if ui
                    .add(Button::image_and_text(params.icons.add.egui(), "New"))
                    .clicked()
                {
                    tasks.0.push(params.pending_task.0.clone());
                }
            });
            // Select new task type
            ComboBox::from_id_source("pending_edit_task")
                .selected_text(params.pending_task.0.label())
                .show_ui(ui, |ui| {
                    for label in Task::<Entity>::labels() {
                        if ui
                            .selectable_label(
                                label == params.pending_task.0.label(),
                                label.to_string(),
                            )
                            .clicked()
                        {
                            *params.pending_task = PendingTask(Task::from_label(label));
                        }
                    }
                });
        });

        ui.add_space(10.0);
        edit_task_component(
            ui,
            &tasks.0.len(),
            &mut params.pending_task.0,
            &params.locations,
            false,
        );
    }
}

/// Returns true if the task should be deleted
fn edit_task_component(
    ui: &mut Ui,
    id: &usize,
    task: &mut Task<Entity>,
    locations: &Query<(Entity, &NameInSite, &LocationTags)>,
    with_delete: bool,
) -> bool {
    let mut is_deleted = false;
    Frame::default()
        .inner_margin(4.0)
        .fill(Color32::DARK_GRAY)
        .rounding(2.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(task.label());

                match task {
                    Task::GoToPlace(location) => {
                        let selected_text = location
                            .0
                            .and_then(|location_entity| {
                                locations
                                    .get(location_entity)
                                    .ok()
                                    .map(|(_, name, _)| name.0.clone())
                            })
                            .unwrap_or("Select Location".to_string());

                        ComboBox::from_id_source(id.to_string() + "select_go_to_location")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                for (entity, name, _) in locations.iter() {
                                    if ui
                                        .selectable_label(
                                            location.0 == Some(entity),
                                            name.0.clone(),
                                        )
                                        .clicked()
                                    {
                                        *location = SiteParent(Some(entity));
                                    }
                                }
                            });
                    }
                    Task::WaitFor(duration) => {
                        ui.add(
                            DragValue::new(duration)
                                .clamp_range(0_f32..=std::f32::INFINITY)
                                .speed(0.01),
                        );
                    }
                }

                // Delete
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if with_delete {
                        if ui.button("❌").on_hover_text("Delete task").clicked() {
                            is_deleted = true;
                        }
                    }
                });
            });
        });
    return is_deleted;
}

#[derive(Resource)]
pub struct PendingTask(Task<Entity>);

impl FromWorld for PendingTask {
    fn from_world(world: &mut World) -> Self {
        PendingTask(Task::GoToPlace(SiteParent::<Entity>(None)))
    }
}

/// When the MobileRobotMarker is added or removed, add or remove the Tasks<Entity> component
fn add_remove_mobile_robot_tasks(
    mut commands: Commands,
    instances: Query<(Entity, Ref<MobileRobotMarker>), Without<Group>>,
    mut removals: RemovedComponents<ModelProperty<MobileRobotMarker>>,
) {
    for removal in removals.read() {
        if instances.get(removal).is_ok() {
            commands.entity(removal).remove::<Tasks<Entity>>();
        }
    }

    for (e, marker) in instances.iter() {
        if marker.is_added() {
            commands.entity(e).insert(Tasks::<Entity>::default());
        }
    }
}
