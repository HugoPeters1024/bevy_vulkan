mod extract;
mod ray_default_plugins;
mod ray_render_plugin;
mod render_device;
mod swapchain;
mod vk_init;
mod vk_utils;

use bevy::prelude::*;

use crate::ray_default_plugins::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.run();
}
