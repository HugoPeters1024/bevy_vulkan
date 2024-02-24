use ash::vk;
use bevy::prelude::*;

use crate::ray_render_plugin::ExtractedWindow;
use crate::render_device::RenderDevice;

#[derive(Resource)]
pub struct Swapchain {
    device: RenderDevice,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub swapchain_format: vk::Format,
    pub swapchain_extent: vk::Extent2D,
    pub current_image_idx: u32,
    pub image_available_semaphore: vk::Semaphore,
    pub render_finished_semaphore: vk::Semaphore,
    pub in_flight_fences: [vk::Fence; 2],
    pub resized: bool,
    pub frame_count: usize,
}

impl Swapchain {
    pub unsafe fn new(device: RenderDevice) -> Self {
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let image_available_semaphore = device
            .device
            .create_semaphore(&semaphore_info, None)
            .unwrap();
        let render_finished_semaphore = device
            .device
            .create_semaphore(&semaphore_info, None)
            .unwrap();

        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let in_flight_fences = [
            device.device.create_fence(&fence_info, None).unwrap(),
            device.device.create_fence(&fence_info, None).unwrap(),
        ];

        Swapchain {
            device,
            swapchain: vk::SwapchainKHR::null(),
            swapchain_images: Vec::new(),
            swapchain_image_views: Vec::new(),
            swapchain_format: vk::Format::UNDEFINED,
            swapchain_extent: vk::Extent2D::default(),
            image_available_semaphore,
            render_finished_semaphore,
            current_image_idx: 0,
            in_flight_fences,
            resized: false,
            frame_count: 0,
        }
    }

    pub unsafe fn on_resize(&mut self, window: &ExtractedWindow) {
        let surface_format = self
            .device
            .ext_surface
            .get_physical_device_surface_formats(self.device.physical_device, self.device.surface)
            .unwrap()[0];
        let surface_caps = self
            .device
            .ext_surface
            .get_physical_device_surface_capabilities(
                self.device.physical_device,
                self.device.surface,
            )
            .unwrap();

        let mut desired_image_count = surface_caps.min_image_count + 1;
        if surface_caps.max_image_count > 0 && desired_image_count > surface_caps.max_image_count {
            desired_image_count = surface_caps.max_image_count;
        }

        let surface_resolution = match surface_caps.current_extent.width {
            std::u32::MAX => vk::Extent2D {
                width: window.width,
                height: window.height,
            },
            _ => surface_caps.current_extent,
        };

        self.swapchain_extent = surface_resolution;

        let pre_transform = if surface_caps
            .supported_transforms
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            surface_caps.current_transform
        };
        let present_modes = self
            .device
            .ext_surface
            .get_physical_device_surface_present_modes(
                self.device.physical_device,
                self.device.surface,
            )
            .unwrap();

        let present_mode = present_modes
            .iter()
            .cloned()
            .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO);

        log::info!("Present mode: {:?}", present_mode);

        let old_swapchain = self.swapchain;
        let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.device.surface)
            .min_image_count(desired_image_count)
            .image_color_space(surface_format.color_space)
            .image_format(surface_format.format)
            .image_extent(surface_resolution)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .image_array_layers(1)
            .old_swapchain(old_swapchain);

        self.swapchain = self
            .device
            .ext_swapchain
            .create_swapchain(&swapchain_create_info, None)
            .unwrap();

        self.device.destroyer.destroy_swapchain(old_swapchain);
        for image_view in self.swapchain_image_views.drain(..) {
            self.device.destroyer.destroy_image_view(image_view);
        }

        self.swapchain_images = self
            .device
            .ext_swapchain
            .get_swapchain_images(self.swapchain)
            .unwrap();

        self.swapchain_image_views = self
            .swapchain_images
            .iter()
            .map(|image| {
                let view_info =
                    crate::vk_init::image_view_info(image.clone(), surface_format.format);
                self.device.create_image_view(&view_info, None).unwrap()
            })
            .collect();

        log::info!(
            "Swapchain created: {}x{} {:?}",
            surface_resolution.width,
            surface_resolution.height,
            surface_format.format
        );
    }

    pub unsafe fn aquire_next_image(
        &mut self,
        window: &ExtractedWindow,
    ) -> (vk::Image, vk::ImageView) {
        if self.swapchain == vk::SwapchainKHR::null() {
            self.on_resize(window);
            self.resized = true;
        }
        self.current_image_idx = self
            .device
            .ext_swapchain
            .acquire_next_image(
                self.swapchain,
                std::u64::MAX,
                self.image_available_semaphore,
                vk::Fence::null(),
            )
            .unwrap()
            .0;

        self.device
            .wait_for_fences(
                std::slice::from_ref(&self.in_flight_fences[self.frame_count % 2]),
                true,
                std::u64::MAX,
            )
            .unwrap();
        self.device
            .reset_fences(std::slice::from_ref(
                &self.in_flight_fences[self.frame_count % 2],
            ))
            .unwrap();

        return (
            self.swapchain_images[self.current_image_idx as usize],
            self.swapchain_image_views[self.current_image_idx as usize],
        );
    }

    pub unsafe fn submit_presentation(
        &mut self,
        window: &ExtractedWindow,
        cmd_buffer: vk::CommandBuffer,
    ) {
        // submit the command buffer to the queue
        let submit_info = vk::SubmitInfo::default()
            .command_buffers(std::slice::from_ref(&cmd_buffer))
            .wait_semaphores(std::slice::from_ref(&self.image_available_semaphore))
            .wait_dst_stage_mask(std::slice::from_ref(
                &vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            ))
            .signal_semaphores(std::slice::from_ref(&self.render_finished_semaphore));

        self.device
            .queue_submit(
                self.device.queue,
                std::slice::from_ref(&submit_info),
                self.in_flight_fences[self.frame_count % 2],
            )
            .unwrap();

        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(std::slice::from_ref(&self.render_finished_semaphore))
            .swapchains(std::slice::from_ref(&self.swapchain))
            .image_indices(std::slice::from_ref(&self.current_image_idx));

        let present_result = self
            .device
            .ext_swapchain
            .queue_present(self.device.queue, &present_info);

        match present_result {
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::SUBOPTIMAL_KHR) => {
                log::info!("------ SWAPCHAIN OUT OF DATE ------");
                self.on_resize(window);
                self.resized = true;
            }
            Err(e) => panic!("Failed to present swapchain image: {:?}", e),
            Ok(_) => {
                self.resized = false;
            }
        }

        self.frame_count += 1;
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        log::info!("Dropping Swapchain");
        unsafe {
            self.device.device_wait_idle().unwrap();

            self.device
                .destroy_semaphore(self.image_available_semaphore, None);
            self.device
                .destroy_semaphore(self.render_finished_semaphore, None);
            for fence in self.in_flight_fences.iter() {
                self.device.destroy_fence(*fence, None);
            }

            for &image_view in self.swapchain_image_views.iter() {
                self.device.destroy_image_view(image_view, None);
            }
            self.device
                .ext_swapchain
                .destroy_swapchain(self.swapchain, None);
        }
    }
}
