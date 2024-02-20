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
use post_process_filter::PostProcessFilter;

use crate::ray_default_plugins::*;

#[derive(Resource)]
struct Keep {
    pipeline: Handle<PostProcessFilter>,
}

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_systems(Startup, setup);
    app.run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(Keep {
        pipeline: asset_server.load("shaders/pprocess.pipeline"),
    });
}
