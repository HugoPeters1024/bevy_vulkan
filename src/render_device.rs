use std::{
    ffi::{c_char, CStr},
    sync::Arc,
};

use ash::extensions::khr;
use ash::vk;
use bevy::{prelude::*, window::RawHandleWrapper};

pub struct RenderDeviceData {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub ext_surface: khr::Surface,
    pub surface: vk::SurfaceKHR,
    pub physical_device: vk::PhysicalDevice,
    pub queue_family_idx: u32,
    pub device: ash::Device,
    pub queue: vk::Queue,
    pub ext_swapchain: khr::Swapchain,
    pub ext_sync2: khr::Synchronization2,
    pub command_pool: vk::CommandPool,
    pub command_buffer: vk::CommandBuffer,
}

impl std::ops::Deref for RenderDeviceData {
    type Target = ash::Device;

    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

#[derive(Resource, Deref)]
pub struct RenderDevice(Arc<RenderDeviceData>);

impl Clone for RenderDevice {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl RenderDevice {
    pub unsafe fn from_window(handles: &RawHandleWrapper) -> Self {
        let entry = ash::Entry::linked();
        let instance = create_instance(handles, &entry);
        let ext_surface = khr::Surface::new(&entry, &instance);
        let surface = create_surface(&entry, &instance, handles);
        let (physical_device, queue_family_idx) =
            pick_physical_device(&instance, &ext_surface, surface);
        let (device, queue) = create_logical_device(&instance, physical_device, queue_family_idx);
        let ext_swapchain = khr::Swapchain::new(&instance, &device);
        let ext_sync2 = khr::Synchronization2::new(&instance, &device);
        let command_pool = create_command_pool(&device, queue_family_idx);
        let command_buffer = create_command_buffer(&device, command_pool);

        RenderDevice(Arc::new(RenderDeviceData {
            entry,
            instance,
            ext_surface,
            surface,
            physical_device,
            queue_family_idx,
            device,
            queue,
            ext_swapchain,
            ext_sync2,
            command_pool,
            command_buffer,
        }))
    }
}

impl Drop for RenderDeviceData {
    fn drop(&mut self) {
        log::info!("Dropping RenderDevice");
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
            self.ext_surface.destroy_surface(self.surface, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

unsafe fn create_instance(window: &RawHandleWrapper, entry: &ash::Entry) -> ash::Instance {
    let app_name = CStr::from_bytes_with_nul_unchecked(b"VK RAYS\0");
    let mut layer_names: Vec<&CStr> = Vec::new();

    #[cfg(debug_assertions)]
    layer_names.push(CStr::from_bytes_with_nul_unchecked(
        b"VK_LAYER_KHRONOS_validation\0",
    ));

    println!("Validation layers:");
    for layer_name in layer_names.iter() {
        println!("  - {}", layer_name.to_str().unwrap());
    }

    let layers_names_raw: Vec<*const c_char> = layer_names
        .iter()
        .map(|raw_name| raw_name.as_ptr())
        .collect();
    let instance_extensions =
        ash_window::enumerate_required_extensions(window.display_handle).unwrap();

    println!("Instance extensions:");
    for extension_name in instance_extensions.iter() {
        println!("  - {}", CStr::from_ptr(*extension_name).to_str().unwrap());
    }

    let app_info = vk::ApplicationInfo::builder()
        .application_name(app_name)
        .application_version(0)
        .engine_name(app_name)
        .engine_version(0)
        .api_version(vk::make_api_version(0, 1, 3, 0));

    let instance_info = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_layer_names(&layers_names_raw)
        .enabled_extension_names(&instance_extensions);

    entry.create_instance(&instance_info, None).unwrap()
}

unsafe fn create_surface(
    entry: &ash::Entry,
    instance: &ash::Instance,
    window: &RawHandleWrapper,
) -> vk::SurfaceKHR {
    ash_window::create_surface(
        &entry,
        &instance,
        window.display_handle,
        window.window_handle,
        None,
    )
    .unwrap()
}

unsafe fn pick_physical_device(
    instance: &ash::Instance,
    ext_surface: &khr::Surface,
    surface: vk::SurfaceKHR,
) -> (vk::PhysicalDevice, u32) {
    let all_devices = instance.enumerate_physical_devices().unwrap();
    println!("Available devices:");
    for device in all_devices.iter() {
        let info = instance.get_physical_device_properties(*device);
        println!(
            "  - {}",
            CStr::from_ptr(info.device_name.as_ptr()).to_str().unwrap()
        );
    }

    let (physical_device, queue_family_idx) = instance
        .enumerate_physical_devices()
        .unwrap()
        .into_iter()
        .find_map(|d| {
            let info = instance.get_physical_device_properties(d);
            if !CStr::from_ptr(info.device_name.as_ptr())
                .to_str()
                .unwrap()
                .contains("NVIDIA")
            {
                return None;
            }

            let properties = instance.get_physical_device_queue_family_properties(d);
            properties.iter().enumerate().find_map(|(i, p)| {
                if p.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && ext_surface
                        .get_physical_device_surface_support(d, i as u32, surface)
                        .unwrap()
                {
                    Some((d, i as u32))
                } else {
                    None
                }
            })
        })
        .expect("Not a single device found!");

    let device_properties = instance.get_physical_device_properties(physical_device);
    println!(
        "Running on device: {}",
        CStr::from_ptr(device_properties.device_name.as_ptr())
            .to_str()
            .unwrap()
    );
    (physical_device, queue_family_idx)
}

unsafe fn create_logical_device(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    queue_family_idx: u32,
) -> (ash::Device, vk::Queue) {
    let device_extensions = [
        khr::Swapchain::name().as_ptr(),
        khr::Synchronization2::name().as_ptr(),
    ];

    println!("Device extensions:");
    for extension_name in device_extensions.iter() {
        println!("  - {}", CStr::from_ptr(*extension_name).to_str().unwrap());
    }

    let queue_info = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(queue_family_idx)
        .queue_priorities(&[1.0])
        .build();

    let mut sync2_info = vk::PhysicalDeviceSynchronization2Features::builder()
        .synchronization2(true)
        .build();

    let mut dynamic_rendering_info = vk::PhysicalDeviceDynamicRenderingFeatures::builder()
        .dynamic_rendering(true)
        .build();

    let mut maintaince4_info = vk::PhysicalDeviceMaintenance4Features::builder()
        .maintenance4(true)
        .build();

    let device_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(std::slice::from_ref(&queue_info))
        .enabled_extension_names(&device_extensions)
        .push_next(&mut sync2_info)
        .push_next(&mut dynamic_rendering_info)
        .push_next(&mut maintaince4_info);

    let device = instance
        .create_device(physical_device, &device_info, None)
        .unwrap();
    let queue = device.get_device_queue(queue_family_idx, 0);

    (device, queue)
}

fn create_command_pool(device: &ash::Device, queue_family_idx: u32) -> vk::CommandPool {
    let pool_info = vk::CommandPoolCreateInfo::builder()
        .queue_family_index(queue_family_idx)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
    unsafe { device.create_command_pool(&pool_info, None).unwrap() }
}

fn create_command_buffer(device: &ash::Device, pool: vk::CommandPool) -> vk::CommandBuffer {
    let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    unsafe {
        device
            .allocate_command_buffers(&command_buffer_allocate_info)
            .unwrap()[0]
    }
}
