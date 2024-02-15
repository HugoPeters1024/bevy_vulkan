mod extract;
mod post_process_filter;
mod ray_default_plugins;
mod ray_render_plugin;
mod render_device;
mod shader;
mod swapchain;
mod vk_init;
mod vk_utils;
mod vulkan_asset;

use bevy::prelude::*;

use crate::ray_default_plugins::*;

#[derive(Resource)]
struct Keep {
    vertex_shader: Handle<crate::shader::Shader>,
    fragment_shader: Handle<crate::shader::Shader>,
}

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_systems(Startup, setup);
    app.run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(Keep {
        vertex_shader: asset_server.load("shaders/quad.vert"),
        fragment_shader: asset_server.load("shaders/quad.frag"),
    });
}
