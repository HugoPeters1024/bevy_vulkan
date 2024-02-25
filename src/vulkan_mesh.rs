use ash::vk;
use bevy::{
    prelude::*,
    render::{mesh::Indices, RenderApp},
};

use crate::{
    extract::Extract,
    render_buffer::{Buffer, BufferProvider},
    render_device::RenderDevice,
    vulkan_asset::{VulkanAsset, VulkanAssetExt},
};

pub struct BLAS {
    pub acceleration_structure: AccelerationStructure,
    pub vertex_buffer: Buffer<u8>,
    pub index_buffer: Buffer<u8>,
}

#[derive(Default)]
pub struct AccelerationStructure {
    pub handle: vk::AccelerationStructureKHR,
    pub buffer: Buffer<u8>,
    pub address: u64,
}

impl AccelerationStructure {
    pub fn get_reference(&self) -> vk::AccelerationStructureReferenceKHR {
        vk::AccelerationStructureReferenceKHR {
            device_handle: self.address,
        }
    }
}

pub fn allocate_acceleration_structure(
    device: &RenderDevice,
    ty: vk::AccelerationStructureTypeKHR,
    build_size: &vk::AccelerationStructureBuildSizesInfoKHR,
) -> AccelerationStructure {
    let buffer: Buffer<u8> = device.create_device_buffer(
        build_size.acceleration_structure_size,
        vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR,
    );

    let acceleration_structure = unsafe {
        device.ext_acc_struct.create_acceleration_structure(
            &vk::AccelerationStructureCreateInfoKHR::default()
                .ty(ty)
                .size(build_size.acceleration_structure_size)
                .buffer(buffer.handle),
            None,
        )
    }
    .unwrap();

    let address = unsafe {
        device
            .ext_acc_struct
            .get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(acceleration_structure),
            )
    };

    AccelerationStructure {
        handle: acceleration_structure,
        buffer,
        address,
    }
}

impl VulkanAsset for Mesh {
    type ExtractedAsset = Mesh;
    type ExtractParam = ();
    type PreparedAsset = BLAS;

    fn extract_asset(
        &self,
        _param: &mut bevy::ecs::system::SystemParamItem<Self::ExtractParam>,
    ) -> Option<Self::ExtractedAsset> {
        Some(self.clone())
    }

    fn prepare_asset(
        asset: Self::ExtractedAsset,
        render_device: &crate::render_device::RenderDevice,
    ) -> Self::PreparedAsset {
        let vertex_count = asset.count_vertices() as u64;
        let index_count = match asset.indices() {
            Some(Indices::U32(indices)) => indices.len() as u64,
            Some(Indices::U16(indices)) => indices.len() as u64,
            None => panic!("Mesh has no indices"),
        };

        log::info!(
            "Building BLAS for mesh with {} vertices and {} indices",
            vertex_count,
            index_count
        );

        let mut vertex_buffer_host: Buffer<u8> = render_device.create_host_buffer(
            vertex_count * 3 * 4,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
        );

        assert!(matches!(asset.indices(), Some(Indices::U32(_))));
        let mut index_buffer_host: Buffer<u8> = render_device.create_host_buffer(
            index_count * 4,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
        );

        {
            let mut vertex_buffer_view = render_device.map_buffer(&mut vertex_buffer_host);
            let mut index_buffer_view = render_device.map_buffer(&mut index_buffer_host);
            vertex_buffer_view.copy_from_slice(asset.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().get_bytes());
            index_buffer_view.copy_from_slice(asset.get_index_buffer_bytes().unwrap());
        }

        let vertex_buffer_device: Buffer<u8> = render_device.create_device_buffer(
            vertex_count * 3 * 4,
            vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        );

        let index_buffer_device: Buffer<u8> = render_device.create_device_buffer(
            index_count * 4,
            vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        );

        render_device.run_transfer_commands(|cmd_buffer| {
            render_device.upload_buffer(cmd_buffer, &vertex_buffer_host, &vertex_buffer_device);
            render_device.upload_buffer(cmd_buffer, &index_buffer_host, &index_buffer_device);
        });

        render_device
            .destroyer
            .destroy_buffer(vertex_buffer_host.handle);
        render_device
            .destroyer
            .destroy_buffer(index_buffer_host.handle);

        let geometry_info = vk::AccelerationStructureGeometryKHR::default()
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
            .geometry(vk::AccelerationStructureGeometryDataKHR {
                triangles: vk::AccelerationStructureGeometryTrianglesDataKHR::default()
                    .vertex_format(vk::Format::R32G32B32_SFLOAT)
                    .vertex_data(vk::DeviceOrHostAddressConstKHR {
                        device_address: vertex_buffer_device.address,
                    })
                    .vertex_stride(12)
                    .max_vertex(0)
                    .index_type(vk::IndexType::UINT32)
                    .index_data(vk::DeviceOrHostAddressConstKHR {
                        device_address: index_buffer_device.address,
                    })
                    .transform_data(vk::DeviceOrHostAddressConstKHR { device_address: 0 }),
            });

        let combined_build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(
                vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
                    | vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION,
            )
            .geometries(std::slice::from_ref(&geometry_info));

        let mut size_info = vk::AccelerationStructureBuildSizesInfoKHR::default();
        unsafe {
            render_device
                .ext_acc_struct
                .get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &combined_build_info,
                    std::slice::from_ref(&((index_count / 3) as u32)),
                    &mut size_info,
                )
        };

        let mut acceleration_structure = allocate_acceleration_structure(
            &render_device,
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            &size_info,
        );

        let scratch_buffer: Buffer<u8> = render_device.create_device_buffer(
            size_info.build_scratch_size,
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
            .primitive_count(index_count as u32 / 3)
            // offset in bytes where the primitive data is defined
            .primitive_offset(0)
            .first_vertex(0)
            .transform_offset(0);

        let build_range_infos = std::slice::from_ref(&build_range_info);

        unsafe {
            render_device.run_transfer_commands(&|cmd_buffer| {
                render_device
                    .ext_acc_struct
                    .cmd_build_acceleration_structures(
                        cmd_buffer,
                        std::slice::from_ref(&build_geometry_info),
                        std::slice::from_ref(&build_range_infos),
                    );
            })
        }

        render_device
            .destroyer
            .destroy_buffer(scratch_buffer.handle);

        acceleration_structure.address = unsafe {
            render_device
                .ext_acc_struct
                .get_acceleration_structure_device_address(
                    &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                        .acceleration_structure(acceleration_structure.handle),
                )
        };

        BLAS {
            acceleration_structure,
            vertex_buffer: vertex_buffer_device,
            index_buffer: index_buffer_device,
        }
    }

    fn destroy_asset(
        render_device: &crate::render_device::RenderDevice,
        prepared_asset: &Self::PreparedAsset,
    ) {
        render_device
            .destroyer
            .destroy_acceleration_structure(prepared_asset.acceleration_structure.handle);
        render_device
            .destroyer
            .destroy_buffer(prepared_asset.acceleration_structure.buffer.handle);
        render_device
            .destroyer
            .destroy_buffer(prepared_asset.vertex_buffer.handle);
        render_device
            .destroyer
            .destroy_buffer(prepared_asset.index_buffer.handle);
    }
}

pub struct VulkanMeshPlugin;

fn extract_meshes(
    mut commands: Commands,
    meshes: Extract<Query<(&Handle<Mesh>, &Transform, &GlobalTransform)>>,
) {
    for (mesh, t, gt) in meshes.iter() {
        commands.spawn((mesh.clone(), t.clone(), gt.clone()));
    }
}

impl Plugin for VulkanMeshPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();

        app.init_vulkan_asset::<Mesh>();

        let render_app = app.get_sub_app_mut(RenderApp).unwrap();
        render_app.add_systems(ExtractSchedule, extract_meshes);
    }
}
