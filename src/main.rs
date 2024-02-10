mod ray_default_plugins;
mod ray_render_plugin;
mod render_device;
mod swapchain;
mod vk_init;
mod vk_utils;
mod extract;

use bevy::prelude::*;
use bevy::render::RenderApp;

use crate::ray_default_plugins::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.run();
}

fn print_thread_id() {
    println!("Thread ID: {:?}", std::thread::current().id());
    std::thread::sleep(std::time::Duration::from_secs(1));
}
