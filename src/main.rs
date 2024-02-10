mod ray_default_plugins;
mod ray_render_plugin;
mod render_device;

use bevy::prelude::*;
use bevy::render::RenderApp;

use crate::ray_default_plugins::*;
use crate::ray_render_plugin::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);

    let render_app = app.get_sub_app_mut(RenderApp).unwrap();
    render_app.add_systems(Render, print_thread_id);

    app.add_systems(Update, print_thread_id);
    app.run();
}

fn print_thread_id() {
    println!("Thread ID: {:?}", std::thread::current().id());
    std::thread::sleep(std::time::Duration::from_secs(1));
}
