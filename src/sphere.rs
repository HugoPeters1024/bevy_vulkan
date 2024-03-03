use ash::vk;
use bevy::{prelude::*, render::RenderApp};

use crate::{
    blas::{allocate_acceleration_structure, AccelerationStructure},
    extract::Extract,
    render_buffer::{Buffer, BufferProvider},
    render_device::RenderDevice,
};

#[derive(Component, Default, Clone)]
pub struct Sphere;

pub struct SpherePlugin;

impl Plugin for SpherePlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(ExtractSchedule, extract_spheres);
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct AABB {
    pub min_x: f32,
    pub min_y: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub max_z: f32,
}

impl Default for AABB {
    fn default() -> Self {
        Self {
            min_x: -0.5,
            min_y: -0.5,
            min_z: -0.5,
            max_x: 0.5,
            max_y: 0.5,
            max_z: 0.5,
        }
    }
}

#[derive(Resource)]
pub struct SphereBLAS {
    pub acceleration_structure: AccelerationStructure,
    pub aabb_buffer: Buffer<AABB>,
}

impl SphereBLAS {
    pub unsafe fn new(device: &RenderDevice) -> Self {
        let mut aabb_buffer_host: Buffer<AABB> = device.create_host_buffer(
            1,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
        );

        {
            let mut aabb_buffer = device.map_buffer(&mut aabb_buffer_host);
            aabb_buffer[0] = AABB::default();
        }

        let aabb_buffer_device: Buffer<AABB> = device.create_device_buffer(
            1,
            vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        );
        device.run_transfer_commands(|cmd_buffer| {
            device.upload_buffer(cmd_buffer, &mut aabb_buffer_host, &aabb_buffer_device);
        });

        device.destroyer.destroy_buffer(aabb_buffer_host.handle);

        let geometry_info = vk::AccelerationStructureGeometryKHR::default()
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry_type(vk::GeometryTypeKHR::AABBS)
            .geometry(vk::AccelerationStructureGeometryDataKHR {
                aabbs: vk::AccelerationStructureGeometryAabbsDataKHR::default()
                    .stride(std::mem::size_of::<AABB>() as u64)
                    .data(vk::DeviceOrHostAddressConstKHR {
                        device_address: aabb_buffer_device.address,
                    }),
            });

        let combined_build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
            .geometries(std::slice::from_ref(&geometry_info));

        let primitive_counts = [1];

        let mut geometry_sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        unsafe {
            device
                .ext_acc_struct
                .get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &combined_build_info,
                    &primitive_counts,
                    &mut geometry_sizes,
                )
        };

        let mut acceleration_structure = allocate_acceleration_structure(
            device,
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            &geometry_sizes,
        );

        let scratch_buffer: Buffer<u8> = device.create_device_buffer(
            geometry_sizes.build_scratch_size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        );

        let build_geometry_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .dst_acceleration_structure(acceleration_structure.handle)
            .geometries(std::slice::from_ref(&geometry_info))
            .scratch_data(vk::DeviceOrHostAddressKHR {
                device_address: scratch_buffer.address,
            });

        let build_range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
            .primitive_count(1)
            // offset in bytes where the primitive data is defined
            .primitive_offset(0)
            .first_vertex(0)
            .transform_offset(0);

        let build_range_infos = std::slice::from_ref(&build_range_info);

        unsafe {
            device.run_transfer_commands(&|cmd_buffer| {
                device.ext_acc_struct.cmd_build_acceleration_structures(
                    cmd_buffer,
                    std::slice::from_ref(&build_geometry_info),
                    std::slice::from_ref(&build_range_infos),
                );
            });

            device.destroyer.destroy_buffer(scratch_buffer.handle);

            acceleration_structure.address = {
                device
                    .ext_acc_struct
                    .get_acceleration_structure_device_address(
                        &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                            .acceleration_structure(acceleration_structure.handle),
                    )
            };
        }

        log::info!("Created sphere BLAS");

        Self {
            acceleration_structure,
            aabb_buffer: aabb_buffer_device,
        }
    }
}

fn extract_spheres(
    mut commands: Commands,
    meshes: Extract<Query<(&Sphere, &Transform, &GlobalTransform)>>,
) {
    for (sphere, t, gt) in meshes.iter() {
        commands.spawn((sphere.clone(), t.clone(), gt.clone()));
    }
}
