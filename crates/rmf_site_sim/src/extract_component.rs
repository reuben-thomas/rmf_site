use bevy::prelude::*;

pub trait ExtractComponent: Component {
    type Out: Component;

    fn extract(&self) -> Self::Out;
}

impl<T: Component + Clone> ExtractComponent for T {
    type Out = T;

    fn extract(&self) -> Self::Out {
        self.clone()
    }
}
