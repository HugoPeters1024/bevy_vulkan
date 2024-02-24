use std::{
    collections::VecDeque,
    ffi::{c_char, CStr},
    sync::{Arc, RwLock},
};

use ash::extensions::khr;
use ash::vk;
use bevy::{prelude::*, utils::HashMap, window::RawHandleWrapper};
use crossbeam::channel::Sender;
use gpu_allocator::{vulkan::*, AllocationError, MemoryLocation};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

const MAX_BINDLESS_IMAGES: u32 = 16536;

pub enum AllocatorState {
    AllocatorState {
        allocator: Allocator,
        image_allocations: HashMap<vk::Image, Allocation>,
        buffer_allocations: HashMap<vk::Buffer, Allocation>,
    },
    AlreadyDropped,
}

impl AllocatorState {
    pub fn allocate(
        &mut self,
        desc: &AllocationCreateDesc<'_>,
    ) -> Result<Allocation, AllocationError> {
        match self {
            Self::AllocatorState { allocator, .. } => allocator.allocate(desc),
            Self::AlreadyDropped => Err(AllocationError::Internal(
                "Allocator already dropped".to_string(),
            )),
        }
    }

    pub fn register_image_allocation(&mut self, image: vk::Image, allocation: Allocation) {
        match self {
            Self::AllocatorState {
                image_allocations, ..
            } => {
                image_allocations.insert(image, allocation);
            }
            Self::AlreadyDropped => panic!("Allocator already dropped"),
        }
    }

    pub fn register_buffer_allocation(&mut self, buffer: vk::Buffer, allocation: Allocation) {
        match self {
            Self::AllocatorState {
                buffer_allocations, ..
            } => {
                buffer_allocations.insert(buffer, allocation);
            }
            Self::AlreadyDropped => panic!("Allocator already dropped"),
        }
    }

    pub fn get_buffer_allocation<'a>(&'a self, buffer: vk::Buffer) -> Option<&'a Allocation> {
        match self {
            Self::AllocatorState {
                buffer_allocations, ..
            } => buffer_allocations.get(&buffer),
            Self::AlreadyDropped => panic!("Allocator already dropped"),
        }
    }

    pub fn free_image_allocation(&mut self, image: vk::Image) {
        match self {
            Self::AllocatorState {
                image_allocations,
                allocator,
                ..
            } => {
                if let Some(allocation) = image_allocations.remove(&image) {
                    allocator.free(allocation).unwrap();
                }
            }
            Self::AlreadyDropped => panic!("Allocator already dropped"),
        }
    }

    pub fn free_buffer_allocation(&mut self, buffer: vk::Buffer) {
        match self {
            Self::AllocatorState {
                buffer_allocations,
                allocator,
                ..
            } => {
                if let Some(allocation) = buffer_allocations.remove(&buffer) {
                    allocator.free(allocation).unwrap();
                }
            }
            Self::AlreadyDropped => panic!("Allocator already dropped"),
        }
    }
}

pub struct RenderDeviceData {
    pub instance: ash::Instance,
    pub ext_surface: khr::Surface,
    pub surface: vk::SurfaceKHR,
    pub physical_device: vk::PhysicalDevice,
    pub queue_family_idx: u32,
    pub device: ash::Device,
    pub queue: vk::Queue,
    pub ext_swapchain: khr::Swapchain,
    pub ext_sync2: khr::Synchronization2,
    pub ext_rtx_pipeline: khr::RayTracingPipeline,
    pub command_pool: vk::CommandPool,
    pub command_buffer: vk::CommandBuffer,
    pub descriptor_pool: vk::DescriptorPool,
    pub destroyer: VkDestroyer,
    pub allocator_state: Arc<RwLock<AllocatorState>>,
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
        let ext_rtx_pipeline = khr::RayTracingPipeline::new(&instance, &device);
        let command_pool = create_command_pool(&device, queue_family_idx);
        let command_buffer = create_command_buffer(&device, command_pool);
        let descriptor_pool = create_descriptor_pool(&device);

        let allocator_state = Arc::new(RwLock::new(AllocatorState::AllocatorState {
            allocator: Allocator::new(&AllocatorCreateDesc {
                instance: instance.clone(),
                device: device.clone(),
                physical_device,
                debug_settings: Default::default(),
                buffer_device_address: true, // Ideally, check the BufferDeviceAddressFeatures struct.
                allocation_sizes: Default::default(),
            })
            .unwrap(),
            image_allocations: HashMap::new(),
            buffer_allocations: HashMap::new(),
        }));

        let destroyer =
            spawn_destroy_thread(instance.clone(), device.clone(), allocator_state.clone());

        RenderDevice(Arc::new(RenderDeviceData {
            instance,
            ext_surface,
            surface,
            physical_device,
            queue_family_idx,
            device,
            queue,
            ext_swapchain,
            ext_sync2,
            ext_rtx_pipeline,
            command_pool,
            command_buffer,
            descriptor_pool,
            destroyer,
            allocator_state,
        }))
    }

    pub fn create_gpu_image(&self, image_info: &vk::ImageCreateInfo) -> vk::Image {
        let image = unsafe { self.device.create_image(image_info, None).unwrap() };
        let requirements = unsafe { self.device.get_image_memory_requirements(image) };

        let mut state = self.allocator_state.write().unwrap();
        let allocation = state
            .allocate(&AllocationCreateDesc {
                name: "Image",
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::DedicatedImage(image),
            })
            .unwrap();

        unsafe {
            self.device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .unwrap();
        }

        state.register_image_allocation(image, allocation);
        image
    }

    pub fn load_shader(
        &self,
        spirv: &[u8],
        stage: vk::ShaderStageFlags,
    ) -> vk::PipelineShaderStageCreateInfo {
        let spirv: &[u32] =
            unsafe { std::slice::from_raw_parts(spirv.as_ptr() as *const u32, spirv.len() / 4) };
        let shader_module = unsafe {
            self.device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spirv), None)
                .unwrap()
        };

        vk::PipelineShaderStageCreateInfo::default()
            .stage(stage)
            .module(shader_module)
            .name(std::ffi::CStr::from_bytes_with_nul(b"main\0").unwrap())
    }
}

impl Drop for RenderDeviceData {
    fn drop(&mut self) {
        log::info!("Dropping RenderDevice");
        unsafe {
            let mut tmp_state = AllocatorState::AlreadyDropped;
            let mut state = self.allocator_state.write().unwrap();
            std::mem::swap(&mut *state, &mut tmp_state);
            drop(tmp_state);
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
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
        ash_window::enumerate_required_extensions(window.get_handle().display_handle().unwrap())
            .unwrap();

    println!("Instance extensions:");
    for extension_name in instance_extensions.iter() {
        println!("  - {}", CStr::from_ptr(*extension_name).to_str().unwrap());
    }

    let app_info = vk::ApplicationInfo::default()
        .application_name(app_name)
        .application_version(0)
        .engine_name(app_name)
        .engine_version(0)
        .api_version(vk::make_api_version(0, 1, 3, 0));

    let instance_info = vk::InstanceCreateInfo::default()
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
        window.get_handle().display_handle().unwrap(),
        window.get_handle().window_handle().unwrap(),
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
        khr::Swapchain::NAME.as_ptr(),
        khr::Synchronization2::NAME.as_ptr(),
        khr::Maintenance4::NAME.as_ptr(),
        khr::AccelerationStructure::NAME.as_ptr(),
        khr::RayTracingPipeline::NAME.as_ptr(),
        khr::DeferredHostOperations::NAME.as_ptr(),
        vk::KhrSpirv14Fn::NAME.as_ptr(),
        vk::ExtDescriptorIndexingFn::NAME.as_ptr(),
    ];

    println!("Device extensions:");
    for extension_name in device_extensions.iter() {
        println!("  - {}", CStr::from_ptr(*extension_name).to_str().unwrap());
    }

    let queue_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family_idx)
        .queue_priorities(&[1.0]);

    let mut sync2_info =
        vk::PhysicalDeviceSynchronization2Features::default().synchronization2(true);

    let mut dynamic_rendering_info =
        vk::PhysicalDeviceDynamicRenderingFeatures::default().dynamic_rendering(true);

    let mut maintaince4_info = vk::PhysicalDeviceMaintenance4Features::default().maintenance4(true);

    let mut bda_info =
        vk::PhysicalDeviceBufferDeviceAddressFeatures::default().buffer_device_address(true);

    let mut features_indexing = vk::PhysicalDeviceDescriptorIndexingFeatures::default()
        .descriptor_binding_partially_bound(true)
        .runtime_descriptor_array(true)
        .descriptor_binding_sampled_image_update_after_bind(true)
        .descriptor_binding_storage_image_update_after_bind(true)
        .descriptor_binding_variable_descriptor_count(true);

    let mut features_acceleration_structure =
        vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default().acceleration_structure(true);

    let mut features_raytracing_pipeline =
        vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::default().ray_tracing_pipeline(true);

    let device_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(std::slice::from_ref(&queue_info))
        .enabled_extension_names(&device_extensions)
        .push_next(&mut sync2_info)
        .push_next(&mut dynamic_rendering_info)
        .push_next(&mut maintaince4_info)
        .push_next(&mut bda_info)
        .push_next(&mut features_indexing)
        .push_next(&mut features_acceleration_structure)
        .push_next(&mut features_raytracing_pipeline);

    let device = instance
        .create_device(physical_device, &device_info, None)
        .unwrap();
    let queue = device.get_device_queue(queue_family_idx, 0);

    (device, queue)
}

fn create_command_pool(device: &ash::Device, queue_family_idx: u32) -> vk::CommandPool {
    let pool_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_family_idx)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
    unsafe { device.create_command_pool(&pool_info, None).unwrap() }
}

fn create_command_buffer(device: &ash::Device, pool: vk::CommandPool) -> vk::CommandBuffer {
    let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    unsafe {
        device
            .allocate_command_buffers(&command_buffer_allocate_info)
            .unwrap()[0]
    }
}

fn create_descriptor_pool(device: &ash::Device) -> vk::DescriptorPool {
    let pool_sizes = [
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1000,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: MAX_BINDLESS_IMAGES,
        },
    ];

    let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
        .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
        .pool_sizes(&pool_sizes)
        .max_sets(1000);

    unsafe {
        device
            .create_descriptor_pool(&descriptor_pool_info, None)
            .unwrap()
    }
}

#[derive(Debug)]
pub enum VkDestroyCmd {
    ImageView(vk::ImageView),
    Image(vk::Image),
    Buffer(vk::Buffer),
    Swapchain(vk::SwapchainKHR),
    Tick,
}

pub struct VkDestroyer {
    sender: Option<Sender<VkDestroyCmd>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl VkDestroyer {
    pub fn destroy_image_view(&self, view: vk::ImageView) {
        self.sender
            .as_ref()
            .unwrap()
            .send(VkDestroyCmd::ImageView(view))
            .unwrap();
    }

    pub fn destroy_image(&self, image: vk::Image) {
        self.sender
            .as_ref()
            .unwrap()
            .send(VkDestroyCmd::Image(image))
            .unwrap();
    }

    pub fn destroy_buffer(&self, buffer: vk::Buffer) {
        self.sender
            .as_ref()
            .unwrap()
            .send(VkDestroyCmd::Buffer(buffer))
            .unwrap();
    }

    pub fn destroy_swapchain(&self, swapchain: vk::SwapchainKHR) {
        self.sender
            .as_ref()
            .unwrap()
            .send(VkDestroyCmd::Swapchain(swapchain))
            .unwrap();
    }

    pub fn tick(&self) {
        self.sender
            .as_ref()
            .unwrap()
            .send(VkDestroyCmd::Tick)
            .unwrap();
    }
}

impl Drop for VkDestroyer {
    fn drop(&mut self) {
        log::info!("Dropping connection to destroy thread");
        let sender = self.sender.take().unwrap();
        drop(sender);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn spawn_destroy_thread(
    instance: ash::Instance,
    device: ash::Device,
    state: Arc<RwLock<AllocatorState>>,
) -> VkDestroyer {
    let ext_swapchain = khr::Swapchain::new(&instance, &device);
    let (sender, receiver) = crossbeam::channel::unbounded();
    let thread = std::thread::spawn(move || {
        // Assuming 2 frames in flight
        let mut queue = VecDeque::from(vec![Vec::new(), Vec::new()]);
        while let Ok(cmd) = receiver.recv() {
            match cmd {
                VkDestroyCmd::Tick => {
                    queue.push_front(Vec::new());
                    let death_list = queue.pop_back().unwrap();
                    for event in death_list {
                        log::info!("Executing destroy {:?}", event);
                        match event {
                            VkDestroyCmd::ImageView(view) => unsafe {
                                device.destroy_image_view(view, None);
                            },
                            VkDestroyCmd::Image(image) => unsafe {
                                let mut state = state.write().unwrap();
                                state.free_image_allocation(image);
                                device.destroy_image(image, None);
                            },
                            VkDestroyCmd::Buffer(buffer) => unsafe {
                                let mut state = state.write().unwrap();
                                state.free_buffer_allocation(buffer);
                                device.destroy_buffer(buffer, None);
                            },
                            VkDestroyCmd::Swapchain(swapchain) => unsafe {
                                ext_swapchain.destroy_swapchain(swapchain, None);
                            },
                            VkDestroyCmd::Tick => panic!("Tick event in death list"),
                        }
                    }
                }
                destroy_event => {
                    queue[0].push(destroy_event);
                }
            }
        }
        log::info!("Destroy thread finished");
    });

    VkDestroyer {
        sender: Some(sender),
        thread: Some(thread),
    }
}
