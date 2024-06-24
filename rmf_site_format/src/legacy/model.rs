use bevy::prelude::default;
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    ops::RangeFrom,
};

use crate::{
    Affiliation, Angle, AssetSource, Group, IsStatic, Model as SiteModel, ModelDescription, ModelDescriptionMarker, ModelInstance, ModelInstanceMarker, ModelMarker, NameInSite, Pose, Rotation, Scale
};

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct Model {
    pub model_name: String,
    #[serde(rename = "name")]
    pub instance_name: String,
    #[serde(rename = "static")]
    pub static_: bool,
    pub x: f64,
    pub y: f64,
    #[serde(rename = "z")]
    pub z_offset: f64,
    pub yaw: f64,
}

impl Model {
    pub fn to_vec(&self) -> DVec2 {
        DVec2::new(self.x, self.y)
    }

    pub fn to_site(&self) -> SiteModel {
        SiteModel {
            name: NameInSite(self.instance_name.clone()),
            source: AssetSource::Search(self.model_name.clone()),
            pose: Pose {
                trans: [self.x as f32, self.y as f32, self.z_offset as f32],
                rot: Rotation::Yaw(Angle::Deg(self.yaw.to_degrees() as f32)),
            },
            is_static: IsStatic(self.static_),
            scale: Scale::default(),
            marker: ModelMarker,
        }
    }

    pub fn to_description(&self) -> ModelDescription {
        ModelDescription {
            name: NameInSite(self.instance_name.clone()),
            source: AssetSource::Search(self.model_name.clone()),
            is_static: IsStatic(self.static_),
            scale: Scale::default(),
            marker: ModelDescriptionMarker,
            group: Group,
        }
    }

    pub fn to_instance(&self,
        model_descriptions: &mut BTreeMap<u32, ModelDescription>, 
        model_description_name_map: &mut HashMap<String, u32>,
        site_id: &mut RangeFrom<u32>,
    ) -> ModelInstance<u32> {
        let description_id = match model_description_name_map.get(&self.model_name) {
            Some(id) => *id,
            None => {
                let new_description_id = site_id.next().unwrap();
                site_id.next();
                model_descriptions.insert(new_description_id, self.to_description());
                model_description_name_map.insert(self.model_name.clone(), new_description_id);
                new_description_id
            }
        };

        ModelInstance {
            name: NameInSite(self.instance_name.clone()),
            pose: Pose {
                trans: [self.x as f32, self.y as f32, self.z_offset as f32],
                rot: Rotation::Yaw(Angle::Deg(self.yaw.to_degrees() as f32)),
            },
            model_description: Affiliation(Some(description_id)),
            marker: ModelInstanceMarker,
        }
    }
}

