use crate::{
    ray_render_plugin::TeardownSchedule, render_buffer::BufferProvider, vk_utils,
    vulkan_mesh::allocate_acceleration_structure,
};
use ash::vk;
use bevy::{prelude::*, render::RenderApp};

use crate::{
    ray_render_plugin::{Render, RenderSet},
    render_buffer::Buffer,
    render_device::RenderDevice,
    vulkan_asset::VulkanAssets,
    vulkan_mesh::AccelerationStructure,
};

#[derive(Default, Resource)]
pub struct TLAS {
    pub acceleration_structure: AccelerationStructure,
    pub address: vk::DeviceAddress,
    pub instance_buffer: Buffer<vk::AccelerationStructureInstanceKHR>,
}

fn update_tlas(
    render_device: Res<RenderDevice>,
    mut tlas: ResMut<TLAS>,
    meshes: Res<VulkanAssets<Mesh>>,
    objects: Query<(&Handle<Mesh>, &GlobalTransform)>,
) {
    let objects = objects.iter().collect::<Vec<_>>();
    let instances: Vec<vk::AccelerationStructureInstanceKHR> = objects
        .iter()
        .filter_map(|(mesh, transform)| {
            let mesh = meshes.get(mesh)?;
            let columns = transform.affine().to_cols_array_2d();
            let transform = vk::TransformMatrixKHR {
                matrix: [
                    columns[0][0],
                    columns[1][0],
                    columns[2][0],
                    columns[3][0],
                    columns[0][1],
                    columns[1][1],
                    columns[2][1],
                    columns[3][1],
                    columns[0][2],
                    columns[1][2],
                    columns[2][2],
                    columns[3][2],
                ],
            };

            let reference = mesh.acceleration_structure.get_reference();
            Some(vk::AccelerationStructureInstanceKHR {
                transform: transform.into(),
                instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xFF),
                instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(0, 0b1),
                acceleration_structure_reference: reference,
            })
        })
        .collect();

    if instances.is_empty() || instances.len() == tlas.instance_buffer.nr_elements as usize {
        return;
    }

    log::info!("Updating TLAS with {} instance(s)", instances.len());

    render_device
        .destroyer
        .destroy_buffer(tlas.instance_buffer.handle);
    tlas.instance_buffer = render_device
        .create_host_buffer::<vk::AccelerationStructureInstanceKHR>(
            instances.len() as u64,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        );

    {
        let mut ptr = render_device.map_buffer(&mut tlas.instance_buffer);
        ptr.copy_from_slice(&instances);
    }

    let geometry = vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::INSTANCES)
        .flags(vk::GeometryFlagsKHR::OPAQUE)
        .geometry(vk::AccelerationStructureGeometryDataKHR {
            instances: vk::AccelerationStructureGeometryInstancesDataKHR::default()
                .array_of_pointers(false)
                .data(vk::DeviceOrHostAddressConstKHR {
                    device_address: tlas.instance_buffer.address,
                }),
        });

    let build_geometry = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        .geometries(std::slice::from_ref(&geometry));

    let primitive_count = instances.len() as u32;

    let mut build_sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
    unsafe {
        render_device
            .ext_acc_struct
            .get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_geometry,
                std::slice::from_ref(&primitive_count),
                &mut build_sizes,
            )
    };

    render_device
        .destroyer
        .destroy_acceleration_structure(tlas.acceleration_structure.handle);
    render_device
        .destroyer
        .destroy_buffer(tlas.acceleration_structure.buffer.handle);
    tlas.acceleration_structure = allocate_acceleration_structure(
        &render_device,
        vk::AccelerationStructureTypeKHR::TOP_LEVEL,
        &build_sizes,
    );

    let as_properties = vk_utils::get_acceleration_structure_properties(&render_device);
    let scratch_alignment =
        as_properties.min_acceleration_structure_scratch_offset_alignment as u64;
    let scratch_size = build_sizes.build_scratch_size + scratch_alignment;

    let scratch_buffer: Buffer<u8> =
        render_device.create_device_buffer(scratch_size, vk::BufferUsageFlags::STORAGE_BUFFER);

    let build_geometry = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
        .dst_acceleration_structure(tlas.acceleration_structure.handle)
        .geometries(std::slice::from_ref(&geometry))
        .scratch_data(vk::DeviceOrHostAddressKHR {
            device_address: scratch_buffer.address + scratch_alignment
                - scratch_buffer.address % scratch_alignment,
        });

    let build_range = vk::AccelerationStructureBuildRangeInfoKHR::default()
        .primitive_count(primitive_count)
        .primitive_offset(0)
        .first_vertex(0)
        .transform_offset(0);

    let build_range_infos = std::slice::from_ref(&build_range);
    unsafe {
        render_device.run_transfer_commands(&|command_buffer| {
            render_device
                .ext_acc_struct
                .cmd_build_acceleration_structures(
                    command_buffer,
                    std::slice::from_ref(&build_geometry),
                    std::slice::from_ref(&build_range_infos),
                );
        });
    }

    render_device
        .destroyer
        .destroy_buffer(scratch_buffer.handle);

    tlas.address = unsafe {
        render_device
            .ext_acc_struct
            .get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(tlas.acceleration_structure.handle),
            )
    };
}

fn cleanup_tlas(world: &mut World) {
    let tlas = world.remove_resource::<TLAS>().unwrap();
    let render_device = world.get_resource::<RenderDevice>().unwrap();
    render_device
        .destroyer
        .destroy_acceleration_structure(tlas.acceleration_structure.handle);
    render_device
        .destroyer
        .destroy_buffer(tlas.acceleration_structure.buffer.handle);
    render_device
        .destroyer
        .destroy_buffer(tlas.instance_buffer.handle);
}

pub struct TLASBuilderPlugin;

impl Plugin for TLASBuilderPlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);

        render_app.init_resource::<TLAS>();
        render_app.add_systems(Render, update_tlas.in_set(RenderSet::Prepare));
        render_app.add_systems(TeardownSchedule, cleanup_tlas);
    }
}
