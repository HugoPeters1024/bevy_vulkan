use std::sync::{Arc, Mutex};

use ash::vk;
use bevy::{
    prelude::*,
    render::RenderApp,
    window::PrimaryWindow,
    winit::{WakeUp, WinitWindows},
};
use egui::{Context, PlatformOutput, RawInput, ViewportId};
use egui_ash_renderer::{DynamicRendering, Options, Renderer};
use winit::event_loop::EventLoop;

use crate::{extract::Extract, ray_render_plugin::TeardownSchedule, render_device::RenderDevice};

pub struct DevUIWorldState {
    pub egui_winit: egui_winit::State,
}

#[derive(Resource)]
pub struct DevUI {
    pub egui_ctx: Context,
    pub renderer: Renderer,
}

#[derive(Resource, Clone, Default)]
pub struct DevUIWorldStateUpdate {
    pub raw_input: RawInput,
}

#[derive(Resource, Clone)]
// the output generated from rendering the egui window
// set by the rendering app, consumed by the main app.
pub struct DevUIPlatformOutput {
    pub platform_output: Arc<Mutex<Option<PlatformOutput>>>,
}

pub struct DevUIPlugin;

impl Plugin for DevUIPlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.get_sub_app(RenderApp).unwrap();
        let render_device = render_app.world().get_resource::<RenderDevice>().unwrap();

        let event_loop = app
            .world()
            .get_non_send_resource::<EventLoop<WakeUp>>()
            .unwrap();

        let egui_ctx = egui::Context::default();

        let egui_winit = egui_winit::State::new(
            egui_ctx.clone(),
            ViewportId::ROOT,
            event_loop,
            None,
            None,
            None,
        );

        let renderer = Renderer::with_default_allocator(
            &render_device.instance,
            render_device.physical_device,
            render_device.device.clone(),
            DynamicRendering {
                color_attachment_format: vk::Format::B8G8R8A8_UNORM,
                depth_attachment_format: None,
            },
            Options {
                srgb_framebuffer: true,
                ..Default::default()
            },
        )
        .unwrap();

        let platform_output = DevUIPlatformOutput {
            platform_output: Arc::new(Mutex::new(None)),
        };

        app.world_mut()
            .insert_non_send_resource(DevUIWorldState { egui_winit });
        app.world_mut().insert_resource(platform_output.clone());
        app.add_systems(Update, (handle_input, handle_output));

        let render_app = app.get_sub_app_mut(RenderApp).unwrap();
        render_app
            .world_mut()
            .init_resource::<DevUIWorldStateUpdate>();
        render_app
            .world_mut()
            .insert_resource(DevUI { egui_ctx, renderer });
        render_app.world_mut().insert_resource(platform_output);
        render_app.add_systems(ExtractSchedule, extract);
        render_app.add_systems(TeardownSchedule, cleanup);
    }
}

fn handle_input(
    mut commands: Commands,
    mut dev_ui_world: NonSendMut<DevUIWorldState>,
    windows: Query<Entity, With<PrimaryWindow>>,
    winit_windows: NonSend<WinitWindows>,
    //mut winit_events: EventReader<RawWinitEvent>,
) {
    if let Ok(window) = windows.get_single() {
        let window = winit_windows.get_window(window).unwrap();
        let raw_input = dev_ui_world.egui_winit.take_egui_input(window);
        commands.insert_resource(DevUIWorldStateUpdate { raw_input });

        // TODO: process winit events, how can we get them?
        //for ev in winit_events.read() {
        //    if ev.0.window_id == window.id {
        //        let _ = dev_ui_world.egui_winit.on_window_event(&window, &ev.0.raw_event);
        //    }
        //}
    }
}

fn extract(mut commands: Commands, world_state: Extract<Res<DevUIWorldStateUpdate>>) {
    commands.insert_resource(world_state.clone());
}

fn handle_output(
    mut dev_ui_world: NonSendMut<DevUIWorldState>,
    windows: Query<Entity, With<PrimaryWindow>>,
    winit_windows: NonSend<WinitWindows>,
    platform_output: Res<DevUIPlatformOutput>,
) {
    if let Ok(window) = windows.get_single() {
        let window = winit_windows.get_window(window).unwrap();
        if let Some(platform_output) = platform_output.platform_output.lock().unwrap().take() {
            dev_ui_world
                .egui_winit
                .handle_platform_output(window, platform_output);
        }
    }
}

fn cleanup(world: &mut World) {
    world.remove_resource::<DevUI>().unwrap();
}
