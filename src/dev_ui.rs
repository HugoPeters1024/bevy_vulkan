use std::{
    ops::{Deref, RangeInclusive},
    sync::{Arc, Mutex},
};

use ash::vk;
use bevy::{
    prelude::*,
    render::RenderApp,
    window::PrimaryWindow,
    winit::{DisplayHandleWrapper, RawWinitWindowEvent, WinitWindows},
};
use egui::{emath, Context, PlatformOutput, RawInput, ViewportId};
use egui_ash_renderer::{DynamicRendering, Options, Renderer};

use crate::{extract::Extract, ray_render_plugin::TeardownSchedule, render_device::RenderDevice};

pub struct DevUIWorldState {
    pub egui_winit: egui_winit::State,
}

#[derive(Clone, Resource)]
pub struct DevUIState {
    pub hidden: bool,
    pub ticks: usize,
    pub fps: f32,
    pub gamma: f32,
    pub exposure: f32,
    pub aperture: f32,
    pub foginess: f32,
    pub fog_scatter: f32,
    pub sky_brightness: f32,
}

impl Default for DevUIState {
    fn default() -> Self {
        Self {
            hidden: false,
            ticks: 0,
            fps: 0.0,
            gamma: 2.4,
            exposure: 1.0,
            aperture: 0.008,
            foginess: 0.001,
            fog_scatter: 0.9,
            sky_brightness: 1.0,
        }
    }
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

impl DevUIState {
    pub fn render(&mut self, ctx: &egui::Context) {
        if self.hidden {
            return;
        }

        egui::Window::new("Dev UI").resizable(true).show(ctx, |ui| {
            ui.label(format!("tick: {}", self.ticks));
            ui.label(format!("fps: {:.2}", self.fps));
            egui::CollapsingHeader::new("Camera")
                .open(Some(true))
                .show(ui, |ui| {
                    Self::slider(ui, "gamma", &mut self.gamma, 1.5..=3.0);
                    Self::slider(ui, "exposure", &mut self.exposure, 0.0..=5.0);
                    Self::slider(ui, "aperture", &mut self.aperture, 0.0..=0.02);
                });
            egui::CollapsingHeader::new("Environment")
                .open(Some(true))
                .show(ui, |ui| {
                    Self::slider(ui, "foginess", &mut self.foginess, 0.0..=0.2);
                    Self::slider(ui, "fog scatter", &mut self.fog_scatter, -1.0..=1.0);
                    Self::slider(ui, "sky_brightness", &mut self.sky_brightness, 0.0..=1.0);
                });
        });
    }

    fn slider<Num: emath::Numeric>(
        ui: &mut egui::Ui,
        text: impl Into<egui::WidgetText>,
        value: &mut Num,
        range: RangeInclusive<Num>,
    ) {
        ui.add(
            egui::Slider::new(value, range)
                .text(text)
                .text_color(egui::Color32::LIGHT_BLUE),
        );
    }
}

pub struct DevUIPlugin;

impl Plugin for DevUIPlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.get_sub_app(RenderApp).unwrap();
        let render_device = render_app.world().get_resource::<RenderDevice>().unwrap();

        let display_handle = app
            .world()
            .get_resource::<DisplayHandleWrapper>()
            .unwrap();

        let egui_ctx = egui::Context::default();

        let egui_winit = egui_winit::State::new(
            egui_ctx.clone(),
            ViewportId::ROOT,
            display_handle.deref(),
            None,
            None,
            None,
        );

        // We won't outlive the render device, so this borrow is okay (tm).
        let allocator = {
            let state = render_device.allocator_state.lock().unwrap();
            state.unchecked_borrow_allocator()
        };

        let renderer = Renderer::with_gpu_allocator(
            allocator,
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
        render_app.world_mut().init_resource::<DevUIState>();
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
    mut winit_events: EventReader<RawWinitWindowEvent>,
) {
    if let Ok(window) = windows.single() {
        let window = winit_windows.get_window(window).unwrap();
        let raw_input = dev_ui_world.egui_winit.take_egui_input(window);
        commands.insert_resource(DevUIWorldStateUpdate { raw_input });

        for ev in winit_events.read() {
            if ev.window_id == window.id() {
                let _ = dev_ui_world.egui_winit.on_window_event(&window, &ev.event);
            }
        }
    }
}

fn extract(
    mut commands: Commands,
    mut ui_state: ResMut<DevUIState>,
    keyboard: Extract<Res<ButtonInput<KeyCode>>>,
    world_state: Extract<Res<DevUIWorldStateUpdate>>,
) {
    if keyboard.just_pressed(KeyCode::Tab) {
        ui_state.hidden = !ui_state.hidden;
    }
    commands.insert_resource(world_state.clone());
}

fn handle_output(
    mut dev_ui_world: NonSendMut<DevUIWorldState>,
    windows: Query<Entity, With<PrimaryWindow>>,
    winit_windows: NonSend<WinitWindows>,
    platform_output: Res<DevUIPlatformOutput>,
) {
    if let Ok(window) = windows.single() {
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
