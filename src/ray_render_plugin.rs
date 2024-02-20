use bevy::{
    app::{AppExit, SubApp},
    ecs::{schedule::ScheduleLabel, system::SystemState},
    prelude::*,
    render::RenderApp,
    window::{PrimaryWindow, RawHandleWrapper, WindowCloseRequested},
};

use ash::vk;

use crate::{extract::Extract, render_device::RenderDevice, vk_utils};

fn close_when_requested(
    mut commands: Commands,
    mut closed: EventReader<WindowCloseRequested>,
    killswitch: Res<WorldToRenderKillSwitch>,
    mut waiting_state: Local<Option<WindowCloseRequested>>,
) {
    match waiting_state.as_ref() {
        None => {
            if let Some(close_event) = closed.read().next() {
                log::info!("Window close requested, sending killswitch to RenderApp");
                killswitch.send_req_close.send(()).unwrap();
                *waiting_state = Some(close_event.clone());
            }
        }
        Some(close_event) => {
            log::info!("Waiting for RenderApp to close...");
            killswitch.recv_res_close.recv().unwrap();
            log::info!("RenderApp has closed, continuing with main app");
            commands.entity(close_event.window).despawn();
        }
    }
}

fn shutdown_render_app(world: &mut World) {
    world.resource_scope(|world, killswitch: Mut<RenderToWorldKillSwitch>| {
        if killswitch.recv_req_close.try_recv().is_ok() {
            log::info!("Received killswitch, shutting down RenderApp");
            world.run_schedule(TeardownSchedule);
            log::info!("RenderApp has shut down, sending ack to main app");
            killswitch.send_res_close.send(()).unwrap();
        }
    });
}

#[derive(ScheduleLabel, PartialEq, Eq, Debug, Clone, Hash)]
pub struct TeardownSchedule;

#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct Render;

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum RenderSet {
    Shutdown,
    ExtractCommands,
    Prepare,
    Render,
    Present,
    Cleanup,
}

impl Render {
    fn base_schedule() -> Schedule {
        let active = |world: &World| world.get_resource::<RenderDevice>().is_some();

        let mut schedule = Schedule::new(Self);
        schedule.configure_sets(
            (
                RenderSet::Shutdown,
                RenderSet::ExtractCommands.run_if(active),
                RenderSet::Prepare.run_if(active),
                RenderSet::Render.run_if(active),
                RenderSet::Present.run_if(active),
                RenderSet::Cleanup,
            )
                .chain(),
        );

        schedule.add_systems(
            (
                apply_deferred.in_set(RenderSet::Shutdown),
                apply_deferred.in_set(RenderSet::ExtractCommands),
                apply_deferred.in_set(RenderSet::Prepare),
                apply_deferred.in_set(RenderSet::Render),
                apply_deferred.in_set(RenderSet::Present),
                apply_deferred.in_set(RenderSet::Cleanup),
            )
                .chain(),
        );

        schedule
    }
}

pub struct RayRenderPlugin;

#[derive(Resource)]
struct WorldToRenderKillSwitch {
    send_req_close: crossbeam::channel::Sender<()>,
    recv_res_close: crossbeam::channel::Receiver<()>,
}

#[derive(Resource)]
struct RenderToWorldKillSwitch {
    send_res_close: crossbeam::channel::Sender<()>,
    recv_req_close: crossbeam::channel::Receiver<()>,
}

impl Plugin for RayRenderPlugin {
    fn build(&self, app: &mut App) {
        let (send_req_close, recv_req_close) = crossbeam::channel::unbounded();
        let (send_res_close, recv_res_close) = crossbeam::channel::unbounded();

        app.world.insert_resource(WorldToRenderKillSwitch {
            send_req_close,
            recv_res_close,
        });

        app.add_systems(Update, close_when_requested);

        let mut render_app = App::empty();

        render_app.main_schedule_label = Render.intern();

        render_app.world.insert_resource(RenderToWorldKillSwitch {
            send_res_close,
            recv_req_close,
        });

        let mut system_state: SystemState<Query<&RawHandleWrapper, With<PrimaryWindow>>> =
            SystemState::new(&mut app.world);
        let query = system_state.get(&app.world);
        let primary_window_handles = query.get_single().unwrap();

        let render_device = unsafe {
            crate::render_device::RenderDevice::from_window(&primary_window_handles.clone())
        };

        let swapchain = unsafe { crate::swapchain::Swapchain::new(render_device.clone()) };

        render_app.add_event::<AppExit>();
        render_app.insert_resource(swapchain);
        render_app.insert_resource(render_device);

        app.init_resource::<ScratchMainWorld>();

        let mut extract_schedule = Schedule::new(ExtractSchedule);
        extract_schedule.set_apply_final_deferred(false);

        let mut teardown_schedule = Schedule::new(TeardownSchedule);
        teardown_schedule.add_systems(on_shutdown);

        render_app.main_schedule_label = Render.intern();
        render_app.add_schedule(extract_schedule);
        render_app.add_schedule(teardown_schedule);
        render_app.add_schedule(Render::base_schedule());

        render_app.add_systems(
            Render,
            apply_extract_commands.in_set(RenderSet::ExtractCommands),
        );

        render_app.add_systems(ExtractSchedule, extract_primary_window);
        render_app.add_systems(
            Render,
            (
                (prepare_frame,).in_set(RenderSet::Prepare),
                (present_frame,).in_set(RenderSet::Present),
                (World::clear_entities).in_set(RenderSet::Cleanup),
                (shutdown_render_app,).in_set(RenderSet::Shutdown),
            ),
        );

        app.insert_sub_app(RenderApp, SubApp::new(render_app, move |main_world, render_app| {
            let total_count = main_world.entities().total_count();

            assert_eq!(
                render_app.world.entities().len(),
                0,
                "An entity was spawned after the entity list was cleared last frame and before the extract schedule began. This is not supported",
            );

            // SAFETY: This is safe given the clear_entities call in the past frame and the assert above
            unsafe {
                render_app
                    .world
                    .entities_mut()
                    .flush_and_reserve_invalid_assuming_no_entities(total_count);
            }

            extract(main_world, render_app);
        }));

        app.init_asset::<crate::shader::Shader>();
        app.init_asset_loader::<crate::shader::ShaderLoader>();
        app.init_asset::<crate::post_process_filter::PostProcessFilter>();
        app.init_asset_loader::<crate::post_process_filter::PostProcessFilterLoader>();
    }
}

#[derive(Resource, Default)]
struct ScratchMainWorld(World);

#[derive(Resource, Default, Deref, DerefMut)]
pub struct MainWorld(World);

fn extract(main_world: &mut World, render_app: &mut App) {
    // temporarily add the app world to the render world as a resource
    let scratch_world = main_world.remove_resource::<ScratchMainWorld>().unwrap();
    let inserted_world = std::mem::replace(main_world, scratch_world.0);
    render_app.world.insert_resource(MainWorld(inserted_world));

    // If the render device is gone, then the render app should be shut down
    if render_app.world.get_resource::<RenderDevice>().is_some() {
        render_app.world.run_schedule(ExtractSchedule);
    }

    // move the app world back, as if nothing happened.
    let inserted_world = render_app.world.remove_resource::<MainWorld>().unwrap();
    let scratch_world = std::mem::replace(main_world, inserted_world.0);
    main_world.insert_resource(ScratchMainWorld(scratch_world));
}

/// Applies the commands from the extract schedule. This happens during
/// the render schedule rather than during extraction to allow the commands to run in parallel with the
/// main app when pipelined rendering is enabled.
fn apply_extract_commands(render_world: &mut World) {
    render_world.resource_scope(|render_world, mut schedules: Mut<Schedules>| {
        schedules
            .get_mut(ExtractSchedule)
            .unwrap()
            .apply_deferred(render_world);
    });
}

#[derive(Resource)]
pub struct ExtractedWindow {
    pub width: u32,
    pub height: u32,
}

fn extract_primary_window(windows: Extract<Query<&Window>>, mut commands: Commands) {
    let Ok(window) = windows.get_single() else {
        return;
    };

    commands.insert_resource(ExtractedWindow {
        width: window.resolution.physical_width().max(1),
        height: window.resolution.physical_height().max(1),
    });
}

#[derive(Resource)]
pub struct Frame {
    pub render_target_image: vk::Image,
    pub render_target: vk::ImageView,
    pub cmd_buffer: vk::CommandBuffer,
}

fn prepare_frame(
    mut commands: Commands,
    render_device: Res<crate::render_device::RenderDevice>,
    window: Res<ExtractedWindow>,
    mut swapchain: ResMut<crate::swapchain::Swapchain>,
) {
    unsafe {
        let (render_target_image, render_target) = swapchain.aquire_next_image(&window);

        commands.insert_resource(Frame {
            render_target_image,
            render_target,
            cmd_buffer: render_device.command_buffer,
        });

        let cmd_buffer = render_device.command_buffer;

        render_device
            .reset_command_buffer(cmd_buffer, vk::CommandBufferResetFlags::empty())
            .unwrap();
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        render_device
            .begin_command_buffer(cmd_buffer, &begin_info)
            .unwrap();

        // Make swapchain available for rendering
        vk_utils::transition_image_layout(
            &render_device,
            cmd_buffer,
            render_target_image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::GENERAL,
        );

        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: swapchain.swapchain_extent,
        };

        let attachment_info = vk::RenderingAttachmentInfoKHR::builder()
            .image_view(render_target)
            .image_layout(vk::ImageLayout::ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
            });

        let render_info = vk::RenderingInfo::builder()
            .layer_count(1)
            .render_area(render_area)
            .color_attachments(std::slice::from_ref(&attachment_info));

        render_device
            .device
            .cmd_begin_rendering(cmd_buffer, &render_info);

        render_device
            .device
            .cmd_set_scissor(cmd_buffer, 0, std::slice::from_ref(&render_area));
        render_device.device.cmd_set_viewport(
            cmd_buffer,
            0,
            std::slice::from_ref(&vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: swapchain.swapchain_extent.width as f32,
                height: swapchain.swapchain_extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }),
        );
    }
}

fn present_frame(
    render_device: Res<crate::render_device::RenderDevice>,
    frame: Res<Frame>,
    window: Res<ExtractedWindow>,
    mut swapchain: ResMut<crate::swapchain::Swapchain>,
) {
    unsafe {
        let cmd_buffer = frame.cmd_buffer;
        render_device.device.cmd_end_rendering(cmd_buffer);

        // Make swapchain available for present
        vk_utils::transition_image_layout(
            &render_device,
            cmd_buffer,
            frame.render_target_image,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );

        render_device.device.end_command_buffer(cmd_buffer).unwrap();
        swapchain.submit_presentation(&window, cmd_buffer);
    }
}

fn on_shutdown(world: &mut World) {
    log::info!("Removing RenderDevice and Swapchain resources");
    world.remove_resource::<crate::swapchain::Swapchain>();
    world.remove_resource::<crate::render_device::RenderDevice>();
}
