use bevy::prelude::*;

#[derive(Component)]
struct PostProcessFilter {
    pub vertex_shader: Handle<crate::shader::Shader>,
    pub fragment_shader: Handle<crate::shader::Shader>,
}
