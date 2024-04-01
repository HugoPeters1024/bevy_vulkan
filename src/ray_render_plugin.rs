use std::fs;

use bevy::{
    app::{AppExit, SubApp},
    ecs::{schedule::ScheduleLabel, system::SystemState},
    prelude::*,
    render::{camera::CameraProjection, RenderApp},
    window::{PrimaryWindow, RawHandleWrapper, WindowCloseRequested, WindowResized},
};

use ash::vk;

use crate::{
    extract::Extract,
    post_process_filter::PostProcessFilter,
    raytracing_pipeline::{RaytracingPipeline, RaytracingPushConstants},
    render_buffer::{Buffer, BufferProvider},
    render_device::RenderDevice,
    sbt::SBT,
    tlas_builder::TLAS,
    vk_init, vk_utils,
    vulkan_asset::VulkanAssets,
};

#[derive(Resource, Default, Clone)]
pub struct RenderConfig {
    pub rtx_pipeline: Handle<RaytracingPipeline>,
    pub postprocess_pipeline: Handle<PostProcessFilter>,
    pub skydome: Handle<bevy::prelude::Image>,
    pub accumulate: bool,
    pub pull_focus: Option<(u32, u32)>,
    pub tick: u32,
}

#[repr(C)]
pub struct UniformData {
    inverse_view: Mat4,
    inverse_projection: Mat4,
    tick: u32,
    accumulate: u32,
    pull_focus_x: u32,
    pull_focus_y: u32,
}

#[repr(C)]
pub struct FocusData {
    focal_distance: f32,
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

fn handle_input(keyboard: Res<ButtonInput<KeyCode>>, mut render_config: ResMut<RenderConfig>) {
    if keyboard.just_pressed(KeyCode::Space) {
        render_config.accumulate = !render_config.accumulate;
    }
}

fn shutdown_render_app(world: &mut World) {
    world.resource_scope(|world, killswitch: Mut<RenderToWorldKillSwitch>| {
        if killswitch.recv_req_close.try_recv().is_ok() {
            log::info!("Received killswitch, shutting down RenderApp");
            let render_device = world.get_resource::<RenderDevice>().unwrap();
            {
                let queue = render_device.queue.lock().unwrap();
                unsafe { render_device.queue_wait_idle(*queue).unwrap() };
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

#[derive(Resource)]
struct BluenoiseBuffer(Buffer<u32>);

impl Plugin for RayRenderPlugin {
    fn build(&self, app: &mut App) {
        let (send_req_close, recv_req_close) = crossbeam::channel::unbounded();
        let (send_res_close, recv_res_close) = crossbeam::channel::unbounded();

        app.world.insert_resource(WorldToRenderKillSwitch {
            send_req_close,
            recv_res_close,
        });

        app.add_systems(
            Update,
            (close_when_requested, handle_input, set_focus_pulling),
        );

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
        let sphere_blas = unsafe { crate::sphere::SphereBLAS::new(&render_device) };
        let bluenoise_buffer = initialize_bluenoise(&render_device);

        render_app.add_event::<AppExit>();
        render_app.add_event::<WindowResized>();
        render_app.insert_resource(swapchain);
        render_app.insert_resource(sphere_blas);
        render_app.insert_resource(bluenoise_buffer);
        render_app.insert_resource(render_device.clone());
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

fn extract_primary_window(
    windows: Extract<Query<&Window>>,
    mut resized_events: Extract<EventReader<WindowResized>>,
    mut write: EventWriter<WindowResized>,
    mut commands: Commands,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };

    commands.insert_resource(ExtractedWindow {
        width: window.resolution.width().max(1.0) as u32,
        height: window.resolution.height().max(1.0) as u32,
    });

    for event in resized_events.read() {
        write.send(event.clone());
    }
}

fn extract_render_config(
    mut commands: Commands,
    render_config: Extract<Res<RenderConfig>>,
    cameras: Extract<Query<(&Camera, &Projection, &Transform, &GlobalTransform)>>,
) {
    commands.insert_resource(render_config.clone());
    for (camera, projection, transform, global_transform) in cameras.iter() {
        commands.spawn((
            camera.clone(),
            projection.clone(),
            transform.clone(),
            global_transform.clone(),
        ));
    }
}

fn set_focus_pulling(
    windows: Query<&Window>,
    mut render_config: ResMut<RenderConfig>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    let window = windows.single();
    render_config.pull_focus = None;

    if let Some(mouse_pos) = window.physical_cursor_position() {
        let x = mouse_pos.x as u32;
        let y = mouse_pos.y as u32;
        if mouse.pressed(MouseButton::Left) {
            render_config.pull_focus = Some((x, y));
        }
    }
}

fn initialize_bluenoise(render_device: &RenderDevice) -> BluenoiseBuffer {
    // load the blue noise data, as lended from lighthouse2
    // https://github.com/jbikker/lighthouse2/blob/e61e65444d8ed3074775003f7aa7d60cb0d4792e/lib/rendercore_optix7/rendercore.cpp#L247
    let sob256_64 = fs::read("assets/sob256_64.raw").unwrap();
    let scr256_64 = fs::read("assets/scr256_64.raw").unwrap();
    let rnk256_64 = fs::read("assets/rnk256_64.raw").unwrap();
    let chunk_len = sob256_64.len();
    log::info!(
        "sob256_64 = ${}, scr256_64 = ${}, rnk256_64 = ${}",
        sob256_64.len(),
        scr256_64.len(),
        rnk256_64.len()
    );
    assert!(
        chunk_len * 5 == sob256_64.len() + scr256_64.len() + rnk256_64.len(),
        "The blue noise data is not the expected size"
    );
    let mut staging_buffer = render_device.create_host_buffer::<u32>(
        5 * sob256_64.len() as u64,
        vk::BufferUsageFlags::TRANSFER_SRC,
    );
    {
        let mut mapped = render_device.map_buffer(&mut staging_buffer);
        for (i, byte) in sob256_64.iter().enumerate() {
            mapped[i] = *byte as u32;
        }

        for (i, byte) in scr256_64.iter().enumerate() {
            mapped[chunk_len * 1 + i] = *byte as u32;
        }

        for (i, byte) in rnk256_64.iter().enumerate() {
            mapped[chunk_len * 3 + i] = *byte as u32;
        }
    }

    let device_buffer = render_device.create_device_buffer::<u32>(
        5 * sob256_64.len() as u64,
        vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::STORAGE_BUFFER,
    );

    render_device.run_transfer_commands(|cmd_buffer| {
        render_device.upload_buffer(cmd_buffer, &staging_buffer, &device_buffer);
    });

    render_device
        .destroyer
        .destroy_buffer(staging_buffer.handle);
    return BluenoiseBuffer(device_buffer);
}

#[derive(Resource, Default)]
pub struct Frame {
    pub swapchain_image: vk::Image,
    pub swapchain_view: vk::ImageView,
    pub render_target_image: vk::Image,
    pub render_target_view: vk::ImageView,
    pub uniform_buffer: Buffer<UniformData>,
    pub focus_data: Buffer<FocusData>,
}

fn render_frame(
    render_device: Res<crate::render_device::RenderDevice>,
    window: Res<ExtractedWindow>,
    mut swapchain: ResMut<crate::swapchain::Swapchain>,
    mut frame: ResMut<Frame>,
    render_config: Res<RenderConfig>,
    rtx_pipelines: Res<VulkanAssets<RaytracingPipeline>>,
    textures: Res<VulkanAssets<bevy::prelude::Image>>,
    postprocess_filters: Res<VulkanAssets<PostProcessFilter>>,
    bluenoise_buffer: Res<BluenoiseBuffer>,
    tlas: Res<TLAS>,
    sbt: Res<SBT>,
    camera: Query<(&Projection, &GlobalTransform), With<Camera>>,
    mut tick: Local<u32>,
) {
    *tick += 1;
    if !render_config.accumulate {
        *tick = 0;
    }
    let camera = camera.single();
    let inverse_view = camera.1.compute_matrix();
    let inverse_projection = match camera.0 {
        Projection::Perspective(perspective) => Mat4::perspective_infinite_reverse_rh(
            perspective.fov,
            (window.width as f32) / (window.height as f32),
            perspective.near,
        )
        .inverse(),
        Projection::Orthographic(orthographic) => orthographic.get_projection_matrix().inverse(),
    };

    // Ensure the uniform_buffer exists
    if frame.uniform_buffer.handle == vk::Buffer::null() {
        frame.uniform_buffer =
            render_device.create_host_buffer(1, vk::BufferUsageFlags::UNIFORM_BUFFER);
    }

    // Ensure the focus_data buffer exists
    if frame.focus_data.handle == vk::Buffer::null() {
        let mut staging_buffer: Buffer<FocusData> = render_device.create_host_buffer(
            1,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
        );

        let initial_data = FocusData {
            focal_distance: 100.0,
        };

        {
            let mut mapped = render_device.map_buffer(&mut staging_buffer);
            mapped.copy_from_slice(&[initial_data]);
        }

        frame.focus_data = render_device.create_device_buffer(
            1,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
        );

        render_device.run_transfer_commands(|cmd_buffer| {
            render_device.upload_buffer(cmd_buffer, &staging_buffer, &frame.focus_data);
        });

        render_device
            .destroyer
            .destroy_buffer(staging_buffer.handle);
    }

    // Update the uniform buffer
    {
        let data = UniformData {
            inverse_view,
            inverse_projection,
            tick: *tick,
            accumulate: if render_config.accumulate { 1 } else { 0 },
            pull_focus_x: render_config
                .pull_focus
                .map(|(x, _)| x)
                .unwrap_or(0xFFFFFFFF),
            pull_focus_y: render_config
                .pull_focus
                .map(|(_, y)| y)
                .unwrap_or(0xFFFFFFFF),
        };

        let mut mapped = render_device.map_buffer(&mut frame.uniform_buffer);
        mapped.copy_from_slice(&[data]);
    }

    unsafe {
        let (swapchain_image, swapchain_view) = swapchain.aquire_next_image(&window);
        render_device.destroyer.tick();
        let cmd_buffer = render_device.command_buffers[swapchain.frame_count % 2];

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
            log::trace!("(Re)creating render target");
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
            if tlas.acceleration_structure.handle != vk::AccelerationStructureKHR::null()
                && sbt.data.address != 0
            {
                // Ensure the descriptor set is up to date
                let render_target_binding = vk::DescriptorImageInfo::default()
                    .image_layout(vk::ImageLayout::GENERAL)
                    .image_view(frame.render_target_view);

                let mut ac_binding = vk::WriteDescriptorSetAccelerationStructureKHR::default()
                    .acceleration_structures(std::slice::from_ref(
                        &tlas.acceleration_structure.handle,
                    ));

                let writes = [
                    vk::WriteDescriptorSet::default()
                        .dst_set(rtx_pipeline.descriptor_sets[swapchain.frame_count % 2])
                        .dst_binding(0)
                        .descriptor_count(1)
                        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                        .image_info(std::slice::from_ref(&render_target_binding)),
                    vk::WriteDescriptorSet::default()
                        .dst_set(rtx_pipeline.descriptor_sets[swapchain.frame_count % 2])
                        .dst_binding(1)
                        .descriptor_count(1)
                        .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                        .push_next(&mut ac_binding),
                ];

                render_device.update_descriptor_sets(&writes, &[]);

                render_device.cmd_bind_descriptor_sets(
                    cmd_buffer,
                    vk::PipelineBindPoint::RAY_TRACING_KHR,
                    rtx_pipeline.pipeline_layout,
                    0,
                    &[
                        rtx_pipeline.descriptor_sets[swapchain.frame_count % 2],
                        render_device.bindless_descriptor_set,
                    ],
                    &[],
                );

                render_device.cmd_bind_pipeline(
                    cmd_buffer,
                    vk::PipelineBindPoint::RAY_TRACING_KHR,
                    rtx_pipeline.pipeline,
                );

                let push_constants = RaytracingPushConstants {
                    uniform_buffer: frame.uniform_buffer.address,
                    material_buffer: tlas.material_buffer.address,
                    bluenoise_buffer: bluenoise_buffer.0.address,
                    focus_buffer: frame.focus_data.address,
                    sky_texture: textures
                        .get(&render_config.skydome)
                        .map_or(0xFFFFFFFF, |t| render_device.register_bindless_texture(&t))
                        as u64,
                };

                render_device.cmd_push_constants(
                    cmd_buffer,
                    rtx_pipeline.pipeline_layout,
                    vk::ShaderStageFlags::ALL,
                    0,
                    bytemuck::cast_slice(&[push_constants]),
                );

                render_device.ext_rtx_pipeline.cmd_trace_rays(
                    cmd_buffer,
                    &sbt.raygen_region,
                    &sbt.miss_region,
                    &sbt.hit_region,
                    &vk::StridedDeviceAddressRegionKHR::default(),
                    swapchain.swapchain_extent.width,
                    swapchain.swapchain_extent.height,
                    1,
                );
            }
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

        let attachment_info = vk::RenderingAttachmentInfo::default()
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

            // Ensure the descriptor set is up to date
            let render_target_binding = vk::DescriptorImageInfo::default()
                .image_layout(vk::ImageLayout::GENERAL)
                .image_view(frame.render_target_view)
                .sampler(render_device.linear_sampler);

            let writes = [vk::WriteDescriptorSet::default()
                .dst_set(pipeline.descriptor_sets[swapchain.frame_count % 2])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&render_target_binding))];

            render_device.update_descriptor_sets(&writes, &[]);

            render_device.cmd_bind_descriptor_sets(
                cmd_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.pipeline_layout,
                0,
                std::slice::from_ref(&pipeline.descriptor_sets[swapchain.frame_count % 2]),
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
    let render_device = world
        .remove_resource::<crate::render_device::RenderDevice>()
        .unwrap();
    let bluenoise_buffer = world.remove_resource::<BluenoiseBuffer>().unwrap();
    render_device
        .destroyer
        .destroy_buffer(bluenoise_buffer.0.handle);

    let frame = world.remove_resource::<Frame>().unwrap();
    render_device
        .destroyer
        .destroy_image_view(frame.render_target_view);
    render_device
        .destroyer
        .destroy_image(frame.render_target_image);
    render_device
        .destroyer
        .destroy_buffer(frame.uniform_buffer.handle);
    render_device
        .destroyer
        .destroy_buffer(frame.focus_data.handle);
    let sphere_blas = world
        .remove_resource::<crate::sphere::SphereBLAS>()
        .unwrap();
    render_device
        .destroyer
        .destroy_buffer(sphere_blas.aabb_buffer.handle);
    sphere_blas.acceleration_structure.destroy(&render_device);

    render_device.destroyer.tick();
    render_device.destroyer.tick();
    render_device.destroyer.tick();
    world.remove_resource::<crate::swapchain::Swapchain>();
}
