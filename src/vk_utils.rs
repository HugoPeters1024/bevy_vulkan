use ash::vk;

use crate::render_device::RenderDevice;

pub struct MaybeThere<T> {
    value: Option<T>,
}

impl<T> MaybeThere<T> {
    pub fn new(value: T) -> Self {
        Self { value: Some(value) }
    }

    pub fn manually_drop(&mut self) -> T {
        self.value.take().expect("Value already dropped")
    }
}

impl<T> std::ops::Deref for MaybeThere<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value.as_ref().expect("Value already dropped")
    }
}

impl<T> std::ops::DerefMut for MaybeThere<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_mut().expect("Value already dropped")
    }
}

impl<T> Drop for MaybeThere<T> {
    fn drop(&mut self) {
        match self.value.take() {
            Some(_) => panic!("MaybeThere should be dropped manually"),
            None => {}
        }
    }
}

pub fn aligned_size(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

pub fn transition_image_layout(
    device: &RenderDevice,
    cmd_buffer: vk::CommandBuffer,
    image: vk::Image,
    from: vk::ImageLayout,
    to: vk::ImageLayout,
) {
    let image_barrier = crate::vk_init::layout_transition2(image, from, to);
    let barrier_info =
        vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&image_barrier));
    unsafe {
        device
            .ext_sync2
            .cmd_pipeline_barrier2(cmd_buffer, &barrier_info);
    }
}

pub fn get_raytracing_properties(
    device: &RenderDevice,
) -> vk::PhysicalDeviceRayTracingPipelinePropertiesKHR {
    let mut raytracing_properties = vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();
    let mut properties2 =
        vk::PhysicalDeviceProperties2KHR::default().push_next(&mut raytracing_properties);
    unsafe {
        device
            .instance
            .get_physical_device_properties2(device.physical_device, &mut properties2)
    }
    raytracing_properties
}

pub fn get_acceleration_structure_properties(
    device: &RenderDevice,
) -> vk::PhysicalDeviceAccelerationStructurePropertiesKHR {
    let mut acceleration_structure_properties =
        vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
    let mut properties2 = vk::PhysicalDeviceProperties2KHR::default()
        .push_next(&mut acceleration_structure_properties);
    unsafe {
        device
            .instance
            .get_physical_device_properties2(device.physical_device, &mut properties2)
    }
    acceleration_structure_properties
}
