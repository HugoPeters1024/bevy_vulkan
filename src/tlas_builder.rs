use crate::{
    blas::BLAS, gltf_mesh::Gltf, ray_render_plugin::TeardownSchedule,
    render_buffer::BufferProvider, sphere::SphereBLAS, vk_utils,
};
use ash::vk;
use bevy::{asset::UntypedAssetId, prelude::*, render::RenderApp, utils::HashMap};

use crate::{
    blas::AccelerationStructure,
    ray_render_plugin::{Render, RenderSet},
    render_buffer::Buffer,
    render_device::RenderDevice,
    vulkan_asset::VulkanAssets,
};

#[derive(Default, Resource)]
pub struct TLAS {
    pub acceleration_structure: AccelerationStructure,
    pub address: vk::DeviceAddress,
    pub instance_buffer: Buffer<vk::AccelerationStructureInstanceKHR>,
    pub scratch_buffer: Buffer<u8>,
    pub mesh_to_hit_offset: HashMap<UntypedAssetId, u32>,
}

impl TLAS {
    pub fn update(
        &mut self,
        render_device: &RenderDevice,
        instances: &[vk::AccelerationStructureInstanceKHR],
    ) {
        // recreate the index buffer if the number of instances changed
        if instances.len() != self.instance_buffer.nr_elements as usize {
            log::info!(
                "Reallocting instance buffer from {} to {} elements",
                self.instance_buffer.nr_elements,
                instances.len()
            );
            render_device
                .destroyer
                .destroy_buffer(self.instance_buffer.handle);
            self.instance_buffer = render_device
                .create_host_buffer::<vk::AccelerationStructureInstanceKHR>(
                    instances.len() as u64,
                    vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
                );
        }

        // update the instance buffer
        {
            let mut ptr = render_device.map_buffer(&mut self.instance_buffer);
            ptr.copy_from_slice(&instances);
        }

        let geometry = vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::INSTANCES)
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR {
                instances: vk::AccelerationStructureGeometryInstancesDataKHR::default()
                    .array_of_pointers(false)
                    .data(vk::DeviceOrHostAddressConstKHR {
                        device_address: self.instance_buffer.address,
                    }),
            });

        let build_geometry = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
            .geometries(std::slice::from_ref(&geometry));

        let primitive_count = self.instance_buffer.nr_elements as u32;
        let mut build_size = vk::AccelerationStructureBuildSizesInfoKHR::default();
        unsafe {
            render_device
                .ext_acc_struct
                .get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &build_geometry,
                    std::slice::from_ref(&primitive_count),
                    &mut build_size,
                )
        };

        // only recreate the buffer for the acceleration_structure if the size changed
        if build_size.acceleration_structure_size != self.acceleration_structure.buffer.nr_elements
        {
            render_device
                .destroyer
                .destroy_buffer(self.acceleration_structure.buffer.handle);
            self.acceleration_structure.buffer = render_device.create_device_buffer(
                build_size.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR,
            );
        }

        render_device
            .destroyer
            .destroy_acceleration_structure(self.acceleration_structure.handle);
        self.acceleration_structure.handle = unsafe {
            render_device.ext_acc_struct.create_acceleration_structure(
                &vk::AccelerationStructureCreateInfoKHR::default()
                    .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
                    .size(build_size.acceleration_structure_size)
                    .buffer(self.acceleration_structure.buffer.handle),
                None,
            )
        }
        .unwrap();

        self.acceleration_structure.address = unsafe {
            render_device
                .ext_acc_struct
                .get_acceleration_structure_device_address(
                    &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                        .acceleration_structure(self.acceleration_structure.handle),
                )
        };

        let as_properties = vk_utils::get_acceleration_structure_properties(&render_device);
        let scratch_alignment =
            as_properties.min_acceleration_structure_scratch_offset_alignment as u64;
        let scratch_size = vk_utils::aligned_size(build_size.build_scratch_size, scratch_alignment);

        // only recreate the scratch buffer if the size changed
        if scratch_size != self.scratch_buffer.nr_elements {
            render_device
                .destroyer
                .destroy_buffer(self.scratch_buffer.handle);
            self.scratch_buffer = render_device
                .create_device_buffer(scratch_size, vk::BufferUsageFlags::STORAGE_BUFFER);
        }

        let scratch_buffer_aligned_address = vk_utils::aligned_size(
            self.scratch_buffer.address,
            as_properties.min_acceleration_structure_scratch_offset_alignment as u64,
        );

        let build_geometry = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .dst_acceleration_structure(self.acceleration_structure.handle)
            .geometries(std::slice::from_ref(&geometry))
            .scratch_data(vk::DeviceOrHostAddressKHR {
                device_address: scratch_buffer_aligned_address,
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
    }
}

pub fn update_tlas(
    render_device: Res<RenderDevice>,
    mut tlas: ResMut<TLAS>,
    meshes: Res<VulkanAssets<Mesh>>,
    gltf_meshes: Res<VulkanAssets<Gltf>>,
    mesh_components: Query<(Entity, &Handle<Mesh>)>,
    gltf_components: Query<(Entity, &Handle<Gltf>)>,
    sphere_blas: Res<SphereBLAS>,
    spheres: Query<(Entity, &crate::sphere::Sphere)>,
    transforms: Query<&GlobalTransform>,
) {
    tlas.mesh_to_hit_offset.clear();
    // Reserve the first offset for the sphere hit group
    let mut offset_counter = 1;

    let mut objects: Vec<(
        Entity,
        Option<UntypedAssetId>,
        vk::AccelerationStructureReferenceKHR,
    )> = Vec::new();
    objects.extend(mesh_components.iter().filter_map(|(e, mesh_handle)| {
        let blas = meshes.get(mesh_handle)?;
        Some((
            e,
            Some(mesh_handle.id().untyped()),
            blas.acceleration_structure.get_reference(),
        ))
    }));
    objects.extend(gltf_components.iter().filter_map(|(e, gltf_handle)| {
        let blas = gltf_meshes.get(gltf_handle)?;
        Some((
            e,
            Some(gltf_handle.id().untyped()),
            blas.acceleration_structure.get_reference(),
        ))
    }));

    for (sphere_e, _) in spheres.iter() {
        objects.push((
            sphere_e,
            None,
            sphere_blas.acceleration_structure.get_reference(),
        ));
    }

    let instances: Vec<vk::AccelerationStructureInstanceKHR> = objects
        .iter()
        .filter_map(|(e, mhandle, reference)| {
            let transform = transforms.get(*e).unwrap();

            let mut offset = 0;
            if let Some(handle) = mhandle {
                offset = offset_counter;
                offset_counter += 1;
                tlas.mesh_to_hit_offset.insert(*handle, offset);
            }

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

            Some(vk::AccelerationStructureInstanceKHR {
                transform: transform.into(),
                instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xFF),
                instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                    offset, 0b1,
                ),
                acceleration_structure_reference: *reference,
            })
        })
        .collect();

    if instances.is_empty() {
        return;
    }

    tlas.update(&render_device, &instances);
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
    render_device
        .destroyer
        .destroy_buffer(tlas.scratch_buffer.handle);
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
