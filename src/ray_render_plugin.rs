use bevy::{
    app::{AppExit, SubApp},
    ecs::{schedule::ScheduleLabel, system::SystemState},
    prelude::*,
    render::RenderApp,
    window::{PrimaryWindow, RawHandleWrapper, WindowCloseRequested},
};

use ash::vk;

use crate::{
    extract::Extract, post_process_filter::PostProcessFilter,
    raytracing_pipeline::RaytracingPipeline, render_device::RenderDevice, vk_init, vk_utils,
    vulkan_asset::VulkanAssets,
};

#[derive(Resource, Default, Clone)]
pub struct RenderConfig {
    pub rtx_pipeline: Handle<RaytracingPipeline>,
    pub postprocess_pipeline: Handle<PostProcessFilter>,
}

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
            let render_device = world.get_resource::<RenderDevice>().unwrap();
            unsafe {
                render_device.device_wait_idle().unwrap();
            }
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
        render_app.world.init_resource::<RenderConfig>();

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
        render_app.init_resource::<Frame>();

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

        render_app.add_systems(
            ExtractSchedule,
            (extract_primary_window, extract_render_config),
        );
        render_app.add_systems(
            Render,
            (
                (render_frame,).in_set(RenderSet::Render),
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
    }
}

#[derive(Resource, Default)]
struct ScratchMainWorld(World);

#[derive(Resource, Default, Deref, DerefMut)]
pub struct MainWorld(pub World);

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

fn extract_render_config(mut commands: Commands, render_config: Extract<Res<RenderConfig>>) {
    commands.insert_resource(render_config.clone());
}

#[derive(Resource, Default)]
pub struct Frame {
    pub swapchain_image: vk::Image,
    pub swapchain_view: vk::ImageView,
    pub render_target_image: vk::Image,
    pub render_target_view: vk::ImageView,
    pub rtx_descriptor_set: vk::DescriptorSet,
    pub postprocess_descriptor_set: vk::DescriptorSet,
}

fn render_frame(
    render_device: Res<crate::render_device::RenderDevice>,
    window: Res<ExtractedWindow>,
    mut swapchain: ResMut<crate::swapchain::Swapchain>,
    mut frame: ResMut<Frame>,
    render_config: Res<RenderConfig>,
    rtx_pipelines: Res<VulkanAssets<RaytracingPipeline>>,
    postprocess_filters: Res<VulkanAssets<PostProcessFilter>>,
) {
    unsafe {
        let (swapchain_image, swapchain_view) = swapchain.aquire_next_image(&window);
        render_device.destroyer.tick();
        let cmd_buffer = render_device.command_buffer;

        frame.swapchain_image = swapchain_image;
        frame.swapchain_view = swapchain_view;

        render_device
            .reset_command_buffer(cmd_buffer, vk::CommandBufferResetFlags::empty())
            .unwrap();

        render_device
            .begin_command_buffer(
                cmd_buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
            .unwrap();

        // (Re)create the render target if needed
        if frame.render_target_image == vk::Image::null() || swapchain.resized {
            log::info!("(Re)creating render target");
            render_device
                .destroyer
                .destroy_image_view(frame.render_target_view);
            render_device
                .destroyer
                .destroy_image(frame.render_target_image);
            let image_info = vk_init::image_info(
                swapchain.swapchain_extent.width,
                swapchain.swapchain_extent.height,
                vk::Format::R32G32B32A32_SFLOAT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
            );
            frame.render_target_image = render_device.create_gpu_image(&image_info);

            let view_info = vk_init::image_view_info(frame.render_target_image, image_info.format);
            frame.render_target_view = render_device.create_image_view(&view_info, None).unwrap();

            // Transition to render target to general
            vk_utils::transition_image_layout(
                &render_device,
                cmd_buffer,
                frame.render_target_image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::GENERAL,
            );
        }

        if let Some(rtx_pipeline) = rtx_pipelines.get(&render_config.rtx_pipeline) {
            // Ensure the descriptor set exists
            if frame.rtx_descriptor_set == vk::DescriptorSet::null() {
                let alloc_info = vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(render_device.descriptor_pool)
                    .set_layouts(std::slice::from_ref(&rtx_pipeline.descriptor_set_layout));
                frame.rtx_descriptor_set =
                    render_device.allocate_descriptor_sets(&alloc_info).unwrap()[0];
            }

            // Ensure the descriptor set is up to date
            let render_target_binding = vk::DescriptorImageInfo::default()
                .image_layout(vk::ImageLayout::GENERAL)
                .image_view(frame.render_target_view);

            let writes = [vk::WriteDescriptorSet::default()
                .dst_set(frame.rtx_descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(std::slice::from_ref(&render_target_binding))];

            render_device.update_descriptor_sets(&writes, &[]);

            render_device.cmd_bind_descriptor_sets(
                cmd_buffer,
                vk::PipelineBindPoint::RAY_TRACING_KHR,
                rtx_pipeline.pipeline_layout,
                0,
                std::slice::from_ref(&frame.rtx_descriptor_set),
                &[],
            );

            render_device.cmd_bind_pipeline(
                cmd_buffer,
                vk::PipelineBindPoint::RAY_TRACING_KHR,
                rtx_pipeline.pipeline,
            );

            render_device.ext_rtx_pipeline.cmd_trace_rays(
                cmd_buffer,
                &rtx_pipeline.shader_binding_table.raygen_region,
                &rtx_pipeline.shader_binding_table.miss_region,
                &rtx_pipeline.shader_binding_table.hit_region,
                &vk::StridedDeviceAddressRegionKHR::default(),
                swapchain.swapchain_extent.width,
                swapchain.swapchain_extent.height,
                1,
            );
        }

        // Make swapchain available for rendering
        vk_utils::transition_image_layout(
            &render_device,
            cmd_buffer,
            swapchain_image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::ATTACHMENT_OPTIMAL,
        );

        let render_area = vk::Rect2D::default().extent(swapchain.swapchain_extent);

        let attachment_info = vk::RenderingAttachmentInfoKHR::default()
            .image_view(swapchain_view)
            .image_layout(vk::ImageLayout::ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE);

        let render_info = vk::RenderingInfo::default()
            .layer_count(1)
            .render_area(render_area)
            .color_attachments(std::slice::from_ref(&attachment_info));

        render_device.cmd_begin_rendering(cmd_buffer, &render_info);

        render_device.cmd_set_scissor(cmd_buffer, 0, std::slice::from_ref(&render_area));
        render_device.cmd_set_viewport(
            cmd_buffer,
            0,
            std::slice::from_ref(
                &vk::Viewport::default()
                    .width(swapchain.swapchain_extent.width as f32)
                    .height(swapchain.swapchain_extent.height as f32)
                    .min_depth(0.0)
                    .max_depth(1.0),
            ),
        );

        if let Some(pipeline) = postprocess_filters.get(&render_config.postprocess_pipeline) {
            render_device.cmd_bind_pipeline(
                cmd_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.pipeline,
            );

            // Ensure the descriptor set exists
            if frame.postprocess_descriptor_set == vk::DescriptorSet::null() {
                let alloc_info = vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(render_device.descriptor_pool)
                    .set_layouts(std::slice::from_ref(&pipeline.descriptor_set_layout));
                frame.postprocess_descriptor_set =
                    render_device.allocate_descriptor_sets(&alloc_info).unwrap()[0];
            }

            // Ensure the descriptor set is up to date
            let render_target_binding = vk::DescriptorImageInfo::default()
                .image_layout(vk::ImageLayout::GENERAL)
                .image_view(frame.render_target_view)
                .sampler(render_device.linear_sampler);

            let writes = [vk::WriteDescriptorSet::default()
                .dst_set(frame.postprocess_descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&render_target_binding))];

            render_device.update_descriptor_sets(&writes, &[]);

            render_device.cmd_bind_descriptor_sets(
                cmd_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.pipeline_layout,
                0,
                std::slice::from_ref(&frame.postprocess_descriptor_set),
                &[],
            );

            render_device.cmd_draw(cmd_buffer, 3, 1, 0, 0);
        }

        render_device.cmd_end_rendering(cmd_buffer);

        // Make swapchain available for present
        vk_utils::transition_image_layout(
            &render_device,
            cmd_buffer,
            frame.swapchain_image,
            vk::ImageLayout::ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );

        render_device.end_command_buffer(cmd_buffer).unwrap();
        swapchain.submit_presentation(&window, cmd_buffer);
    }
}

fn on_shutdown(world: &mut World) {
    log::info!("Removing RenderDevice and Swapchain resources");
    let render_device = world
        .remove_resource::<crate::render_device::RenderDevice>()
        .unwrap();
    let frame = world.remove_resource::<Frame>().unwrap();
    render_device
        .destroyer
        .destroy_image_view(frame.render_target_view);
    render_device
        .destroyer
        .destroy_image(frame.render_target_image);
    render_device.destroyer.tick();
    render_device.destroyer.tick();
    render_device.destroyer.tick();
    world.remove_resource::<crate::swapchain::Swapchain>();
}
