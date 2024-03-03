use ash::vk;
use bytemuck::{Pod, Zeroable};

use crate::{
    render_buffer::{Buffer, BufferProvider},
    render_device::RenderDevice,
};

#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Debug)]
pub struct GeometryDescr {
    pub first_vertex: usize,
    pub vertex_count: usize,
    pub first_index: usize,
    pub index_count: usize,
}

pub struct BLAS {
    pub acceleration_structure: AccelerationStructure,
    pub vertex_buffer: Buffer<u8>,
    pub index_buffer: Buffer<u8>,
    pub geometry_to_index: Vec<u32>,
}

impl BLAS {
    pub fn destroy(&self, render_device: &RenderDevice) {
        render_device
            .destroyer
            .destroy_acceleration_structure(self.acceleration_structure.handle);
        render_device
            .destroyer
            .destroy_buffer(self.acceleration_structure.buffer.handle);
        render_device
            .destroyer
            .destroy_buffer(self.vertex_buffer.handle);
        render_device
            .destroyer
            .destroy_buffer(self.index_buffer.handle);
    }
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

pub fn build_blas_from_buffers(
    render_device: &RenderDevice,
    vertex_count: usize,
    index_count: usize,
    vertex_buffer: &[u8],
    index_buffer: &[u8],
    geometries: &[GeometryDescr],
) -> BLAS {
    log::info!(
        "Building BLAS for mesh with {} vertices and {} indices and {} geometries",
        vertex_count,
        index_count,
        geometries.len()
    );

    let mut vertex_buffer_host: Buffer<u8> = render_device.create_host_buffer(
        std::mem::size_of::<Vertex>() as u64 * vertex_count as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
    );

    let mut index_buffer_host: Buffer<u8> = render_device.create_host_buffer(
        index_count as u64 * 4,
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
    );

    {
        let mut vertex_buffer_view = render_device.map_buffer(&mut vertex_buffer_host);
        let mut index_buffer_view = render_device.map_buffer(&mut index_buffer_host);
        vertex_buffer_view.copy_from_slice(vertex_buffer);
        index_buffer_view.copy_from_slice(index_buffer);
    }

    let vertex_buffer_device: Buffer<u8> = render_device.create_device_buffer(
        std::mem::size_of::<Vertex>() as u64 * vertex_count as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
    );

    let index_buffer_device: Buffer<u8> = render_device.create_device_buffer(
        index_count as u64 * 4,
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

    //let geometry_info = vk::AccelerationStructureGeometryKHR::default()
    //    .flags(vk::GeometryFlagsKHR::OPAQUE)
    //    .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
    //    .geometry(vk::AccelerationStructureGeometryDataKHR {
    //        triangles: vk::AccelerationStructureGeometryTrianglesDataKHR::default()
    //            .vertex_format(vk::Format::R32G32B32_SFLOAT)
    //            .vertex_data(vk::DeviceOrHostAddressConstKHR {
    //                device_address: vertex_buffer_device.address,
    //            })
    //            .vertex_stride(std::mem::size_of::<Vertex>() as u64)
    //            .max_vertex(0)
    //            .index_type(vk::IndexType::UINT32)
    //            .index_data(vk::DeviceOrHostAddressConstKHR {
    //                device_address: index_buffer_device.address,
    //            })
    //            .transform_data(vk::DeviceOrHostAddressConstKHR { device_address: 0 }),
    //    });

    let geometry_infos = geometries
        .iter()
        .map(|geometry| {
            vk::AccelerationStructureGeometryKHR::default()
                .flags(vk::GeometryFlagsKHR::OPAQUE)
                .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
                .geometry(vk::AccelerationStructureGeometryDataKHR {
                    triangles: vk::AccelerationStructureGeometryTrianglesDataKHR::default()
                        .vertex_format(vk::Format::R32G32B32_SFLOAT)
                        .vertex_data(vk::DeviceOrHostAddressConstKHR {
                            device_address: vertex_buffer_device.address,
                        })
                        .vertex_stride(std::mem::size_of::<Vertex>() as u64)
                        .max_vertex(0)
                        .index_type(vk::IndexType::UINT32)
                        .index_data(vk::DeviceOrHostAddressConstKHR {
                            device_address: index_buffer_device.address,
                        })
                        .transform_data(vk::DeviceOrHostAddressConstKHR { device_address: 0 }),
                })
        })
        .collect::<Vec<_>>();

    let combined_build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
        .flags(
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
                | vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION,
        )
        .geometries(&geometry_infos);

    let primitive_counts = geometries
        .iter()
        .map(|geometry| (geometry.index_count / 3) as u32)
        .collect::<Vec<_>>();

    let mut size_info = vk::AccelerationStructureBuildSizesInfoKHR::default();
    unsafe {
        render_device
            .ext_acc_struct
            .get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &combined_build_info,
                &primitive_counts,
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
        .geometries(&geometry_infos)
        .scratch_data(vk::DeviceOrHostAddressKHR {
            device_address: scratch_buffer.address,
        });

    let build_range_infos: Vec<vk::AccelerationStructureBuildRangeInfoKHR> = geometries
        .iter()
        .map(|geometry| {
            vk::AccelerationStructureBuildRangeInfoKHR::default()
                .primitive_count((geometry.index_count / 3) as u32)
                // offset in bytes where the primitive data is defined
                .primitive_offset(geometry.first_index as u32 * std::mem::size_of::<u32>() as u32)
                .first_vertex(0)
                .transform_offset(0)
        })
        .collect();

    let singleton_build_ranges = &[build_range_infos.as_slice()];

    unsafe {
        render_device.run_transfer_commands(&|cmd_buffer| {
            render_device
                .ext_acc_struct
                .cmd_build_acceleration_structures(
                    cmd_buffer,
                    std::slice::from_ref(&build_geometry_info),
                    singleton_build_ranges,
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

    let index_offsets = geometries
        .iter()
        .map(|geometry| geometry.first_index as u32)
        .collect();

    dbg!(&index_offsets);

    BLAS {
        acceleration_structure,
        vertex_buffer: vertex_buffer_device,
        index_buffer: index_buffer_device,
        geometry_to_index: index_offsets,
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
