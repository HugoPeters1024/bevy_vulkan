use bevy::{
    app::{AppExit, SubApp},
    ecs::schedule::ScheduleLabel,
    prelude::*,
    render::RenderApp,
    window::{RawHandleWrapperHolder, WindowCloseRequested, WindowResized},
    winit::WakeUp,
};
use raw_window_handle::HasDisplayHandle;
use winit::event_loop::EventLoop;

use ash::vk;

use crate::{
    bluenoise_plugin::BlueNoiseBuffer,
    extract::Extract,
    post_process_filter::PostProcessFilter,
    raytracing_pipeline::{RaytracingPipeline, RaytracingPushConstants},
    render_buffer::{Buffer, BufferProvider},
    render_device::{RenderDevice, WHITE_TEXTURE_IDX},
    sbt::SBT,
    tlas_builder::TLAS,
    vk_init, vk_utils,
    vulkan_asset::VulkanAssets,
};

#[derive(Resource, Clone)]
pub struct RenderConfig {
    pub rtx_pipeline: Handle<RaytracingPipeline>,
    pub postprocess_pipeline: Handle<PostProcessFilter>,
    pub skydome: Option<Handle<bevy::prelude::Image>>,
    pub sky_color: Vec4,
    pub accumulate: bool,
    pub pull_focus: Option<(u32, u32)>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            rtx_pipeline: Default::default(),
            postprocess_pipeline: Default::default(),
            skydome: Default::default(),
            sky_color: Vec4::splat(1.0),
            accumulate: Default::default(),
            pull_focus: Default::default(),
        }
    }
}

#[repr(C)]
pub struct UniformData {
    sky_color: Vec4,
    inverse_view: Mat4,
    inverse_projection: Mat4,
    tick: u32,
    accumulate: u32,
    pull_focus_x: u32,
    pull_focus_y: u32,
    gamma: f32,
    exposure: f32,
    aperture: f32,
    foginess: f32,
    fog_scatter: f32,
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

impl Plugin for RayRenderPlugin {
    fn build(&self, app: &mut App) {
        let (send_req_close, recv_req_close) = crossbeam::channel::unbounded();
        let (send_res_close, recv_res_close) = crossbeam::channel::unbounded();

        app.world_mut().insert_resource(WorldToRenderKillSwitch {
            send_req_close,
            recv_res_close,
        });

        app.add_systems(
            Update,
            (close_when_requested, handle_input, set_focus_pulling),
        );

        let mut render_app = SubApp::new();
        render_app.update_schedule = Some(Render.intern());

        render_app
            .world_mut()
            .insert_resource(RenderToWorldKillSwitch {
                send_res_close,
                recv_req_close,
            });
        render_app.world_mut().init_resource::<RenderConfig>();

        let event_loop = app
            .world()
            .get_non_send_resource::<EventLoop<WakeUp>>()
            .unwrap();

        let render_device = unsafe {
            crate::render_device::RenderDevice::from_display(
                &event_loop.owned_display_handle().display_handle().unwrap(),
            )
        };

        let sphere_blas = unsafe { crate::sphere::SphereBLAS::new(&render_device) };

        render_app.add_event::<AppExit>();
        render_app.add_event::<WindowResized>();
        render_app.insert_resource(sphere_blas);
        render_app.insert_resource(render_device.clone());
        render_app.init_resource::<Frame>();

        app.init_resource::<ScratchMainWorld>();

        let extract_schedule = Schedule::new(ExtractSchedule);
        let mut teardown_schedule = Schedule::new(TeardownSchedule);
        teardown_schedule.add_systems(on_shutdown);

        render_app.add_schedule(extract_schedule);
        render_app.add_schedule(teardown_schedule);
        render_app.add_schedule(Render::base_schedule());

        render_app.add_systems(
            Render,
            apply_extract_commands.in_set(RenderSet::ExtractCommands),
        );

        render_app.add_systems(
            ExtractSchedule,
            (extract_time, extract_primary_window, extract_render_config),
        );
        render_app.add_systems(
            Render,
            (
                (render_frame).in_set(RenderSet::Render),
                (World::clear_entities).in_set(RenderSet::Cleanup),
                (shutdown_render_app,).in_set(RenderSet::Shutdown),
            )
                .run_if(run_if_render_device_exists),
        );

        render_app.set_extract(|main_world, render_world| {
            let total_count = main_world.entities().total_count();

            assert_eq!(
                render_world.entities().len(),
                0,
                "An entity was spawned after the entity list was cleared last frame and before the extract schedule began. This is not supported",
            );

            // SAFETY: This is safe given the clear_entities call in the past frame and the assert above
            unsafe {
                render_world
                    .entities_mut()
                    .flush_and_reserve_invalid_assuming_no_entities(total_count);
            }

            extract(main_world, render_world);
        });
        app.insert_sub_app(RenderApp, render_app);
    }
}

#[derive(Resource, Default)]
struct ScratchMainWorld(World);

#[derive(Resource, Default, Deref, DerefMut)]
pub struct MainWorld(pub World);

fn extract(main_world: &mut World, render_world: &mut World) {
    // temporarily add the app world to the render world as a resource
    let scratch_world = main_world.remove_resource::<ScratchMainWorld>().unwrap();
    let inserted_world = std::mem::replace(main_world, scratch_world.0);
    render_world.insert_resource(MainWorld(inserted_world));

    // If the render device is gone, then the render app should be shut down
    if render_world.get_resource::<RenderDevice>().is_some() {
        render_world.run_schedule(ExtractSchedule);
    }

    // move the app world back, as if nothing happened.
    let inserted_world = render_world.remove_resource::<MainWorld>().unwrap();
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
    windows: Extract<Query<(&Window, &RawHandleWrapperHolder)>>,
    mut resized_events: Extract<EventReader<WindowResized>>,
    mut write: EventWriter<WindowResized>,
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    swapchain: Option<Res<crate::swapchain::Swapchain>>,
) {
    let Ok((window, handle_holder)) = windows.get_single() else {
        return;
    };

    // initialize the swapchain if it isn't already
    if swapchain.is_none() {
        let handle_holder = handle_holder.0.lock().unwrap();
        if let Some(handles) = &*handle_holder {
            commands.insert_resource(unsafe {
                crate::swapchain::Swapchain::from_window(render_device.clone(), &handles)
            });
        }
    }

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
    cameras: Extract<
        Query<(
            &Camera,
            &Camera3d,
            &Projection,
            &Transform,
            &GlobalTransform,
        )>,
    >,
) {
    commands.insert_resource(render_config.clone());
    for (camera, camera3d, projection, transform, global_transform) in cameras.iter() {
        commands.spawn((
            camera.clone(),
            camera3d.clone(),
            projection.clone(),
            transform.clone(),
            global_transform.clone(),
        ));
    }
}

fn extract_time(mut commands: Commands, time: Extract<Res<Time>>) {
    commands.insert_resource(time.clone());
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

#[derive(Resource, Default)]
pub struct Frame {
    pub swapchain_image: vk::Image,
    pub swapchain_view: vk::ImageView,
    pub render_frame_buffers: RenderFrameBuffers,
    pub uniform_buffer: Buffer<UniformData>,
    pub focus_data: Buffer<FocusData>,
}

#[derive(Default)]
pub struct RenderFrameBuffers {
    pub main: (vk::Image, vk::ImageView),
}

impl RenderFrameBuffers {
    pub unsafe fn prepare(
        &mut self,
        render_device: &RenderDevice,
        swapchain: &crate::swapchain::Swapchain,
        cmd_buffer: vk::CommandBuffer,
    ) {
        // (Re)create the render target if needed
        if self.main.0 == vk::Image::null() || swapchain.resized {
            log::trace!("(Re)creating render target");
            render_device.destroyer.destroy_image_view(self.main.1);
            render_device.destroyer.destroy_image(self.main.0);
            let image_info = vk_init::image_info(
                swapchain.swapchain_extent.width,
                swapchain.swapchain_extent.height,
                vk::Format::R32G32B32A32_SFLOAT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
            );
            self.main.0 = render_device.create_gpu_image(&image_info);

            let view_info = vk_init::image_view_info(self.main.0, image_info.format);
            self.main.1 = render_device.create_image_view(&view_info, None).unwrap();

            // Transition to render target to general
            vk_utils::transition_image_layout(
                &render_device,
                cmd_buffer,
                self.main.0,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::GENERAL,
            );
        }
    }

    pub fn destroy(&mut self, render_device: &RenderDevice) {
        render_device.destroyer.destroy_image_view(self.main.1);
        render_device.destroyer.destroy_image(self.main.0);
    }
}

fn render_frame(
    render_device: Res<crate::render_device::RenderDevice>,
    window: Res<ExtractedWindow>,
    swapchain: Option<ResMut<crate::swapchain::Swapchain>>,
    dev_ui_stuff: (
        Option<ResMut<crate::dev_ui::DevUI>>,
        Option<ResMut<crate::dev_ui::DevUIState>>,
        Option<Res<crate::dev_ui::DevUIWorldStateUpdate>>,
        Option<Res<crate::dev_ui::DevUIPlatformOutput>>,
    ),
    mut frame: ResMut<Frame>,
    render_config: Res<RenderConfig>,
    rtx_pipelines: Res<VulkanAssets<RaytracingPipeline>>,
    textures: Res<VulkanAssets<bevy::prelude::Image>>,
    postprocess_filters: Res<VulkanAssets<PostProcessFilter>>,
    bluenoise_buffer: Res<BlueNoiseBuffer>,
    tlas: Res<TLAS>,
    sbt: Res<SBT>,
    camera: Query<(&Projection, &GlobalTransform), With<Camera>>,
    mut tick: Local<u32>,
    time: Res<Time>,
    mut fps_runnig_avg: Local<f32>,
) {
    let Some(mut swapchain) = swapchain else {
        return;
    };

    let (
        Some(mut dev_ui),
        Some(mut dev_ui_state),
        Some(dev_ui_update),
        Some(dev_ui_platform_output),
    ) = dev_ui_stuff
    else {
        return;
    };

    *tick += 1;
    if !render_config.accumulate {
        *tick = 0;
    }
    let camera = camera.single();
    let inverse_view = camera.1.compute_matrix();
    let projection_matrix = match camera.0 {
        Projection::Perspective(perspective) => Mat4::perspective_infinite_reverse_rh(
            perspective.fov,
            (window.width as f32) / (window.height as f32),
            perspective.near,
        ),
        Projection::Orthographic(_) => todo!("orthographic camera"),
    };
    let inverse_projection = projection_matrix.inverse();

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
            sky_color: render_config.sky_color,
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
            gamma: dev_ui_state.gamma,
            exposure: dev_ui_state.exposure,
            aperture: dev_ui_state.aperture,
            foginess: dev_ui_state.foginess,
            fog_scatter: dev_ui_state.fog_scatter,
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

        frame
            .render_frame_buffers
            .prepare(&render_device, &swapchain, cmd_buffer);

        if let Some(rtx_pipeline) = rtx_pipelines.get(&render_config.rtx_pipeline) {
            if tlas.acceleration_structure.handle != vk::AccelerationStructureKHR::null()
                && sbt.data.address != 0
            {
                // Ensure the descriptor set is up to date
                let render_target_main_binding = vk::DescriptorImageInfo::default()
                    .image_layout(vk::ImageLayout::GENERAL)
                    .image_view(frame.render_frame_buffers.main.1);

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
                        .image_info(std::slice::from_ref(&render_target_main_binding)),
                    vk::WriteDescriptorSet::default()
                        .dst_set(rtx_pipeline.descriptor_sets[swapchain.frame_count % 2])
                        .dst_binding(100)
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
                    bluenoise_buffer2: bluenoise_buffer.0.address,
                    focus_buffer: frame.focus_data.address,
                    sky_texture: match &render_config.skydome {
                        None => WHITE_TEXTURE_IDX,
                        Some(skydome) => textures.get(skydome).map_or(WHITE_TEXTURE_IDX, |t| {
                            render_device.register_bindless_texture(&t)
                        }),
                    },
                    padding: [0; 1],
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

            let push_constants = frame.uniform_buffer.address;
            render_device.cmd_push_constants(
                cmd_buffer,
                pipeline.pipeline_layout,
                vk::ShaderStageFlags::ALL,
                0,
                bytemuck::cast_slice(&[push_constants]),
            );

            // Ensure the descriptor set is up to date
            let render_target_main_binding = vk::DescriptorImageInfo::default()
                .image_layout(vk::ImageLayout::GENERAL)
                .image_view(frame.render_frame_buffers.main.1)
                .sampler(render_device.linear_sampler);

            let writes = [vk::WriteDescriptorSet::default()
                .dst_set(pipeline.descriptor_sets[swapchain.frame_count % 2])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&render_target_main_binding))];

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

        // render the egui dev ui
        let raw_input = dev_ui_update.raw_input.clone();

        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            ..
        } = dev_ui.egui_ctx.run(raw_input, |ctx| {
            dev_ui_state.ticks = *tick as usize;
            // no idea why the running average starts at inf.
            if *fps_runnig_avg > 100000.0 {
                *fps_runnig_avg = 0.0;
            }
            *fps_runnig_avg = 0.95 * *fps_runnig_avg + 0.05 * (1.0 / time.delta_secs());
            dev_ui_state.fps = *fps_runnig_avg;
            dev_ui_state.render(ctx);
        });

        // send the platform output to the main app for processing
        {
            let mut platform_output_slot = dev_ui_platform_output.platform_output.lock().unwrap();
            *platform_output_slot = Some(platform_output);
        }

        dev_ui.renderer.free_textures(&textures_delta.free).unwrap();
        if !textures_delta.set.is_empty() {
            let queue = render_device.queue.lock().unwrap();
            dev_ui
                .renderer
                .set_textures(
                    *queue,
                    render_device.command_pool,
                    textures_delta.set.as_slice(),
                )
                .expect("Failed to update texture");
        }

        let clipped_primitives = dev_ui.egui_ctx.tessellate(shapes, pixels_per_point);

        dev_ui
            .renderer
            .cmd_draw(
                cmd_buffer,
                swapchain.swapchain_extent,
                pixels_per_point,
                &clipped_primitives,
            )
            .unwrap();

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

    let mut frame = world.remove_resource::<Frame>().unwrap();
    frame.render_frame_buffers.destroy(&render_device);

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

fn run_if_render_device_exists(device: Option<Res<RenderDevice>>) -> bool {
    device.is_some()
}
