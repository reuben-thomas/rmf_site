use crate::{
    Angle, AssetSource, IsStatic, Model as SiteModel, ModelDescription,
    ModelDescriptionMarker, ModelInstance, ModelInstanceBundle, ModelInstanceMarker, ModelMarker,
    NameInSite, Pose, Rotation, Scale,
};
use glam::DVec2;
use serde::{Deserialize, Serialize};

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
            ..Default::default()
        }
    }

    pub fn to_instance(
        &self,
        parent_id: u32,
        model_description_id: u32,
        number: &u32,
    ) -> ModelInstance {
        ModelInstance {
            parent: parent_id,
            model_description: model_description_id,
            bundle: ModelInstanceBundle {
                name: NameInSite(self.instance_name.clone() + " #" + &number.to_string()),
                source: AssetSource::Search(self.model_name.clone()),
                pose: Pose {
                    trans: [self.x as f32, self.y as f32, self.z_offset as f32],
                    rot: Rotation::Yaw(Angle::Deg(self.yaw.to_degrees() as f32)),
                },
                scale: Scale::default(),
                is_static: IsStatic(self.static_),
                marker: ModelInstanceMarker,
                ..Default::default()
            },
        }
    }
}
