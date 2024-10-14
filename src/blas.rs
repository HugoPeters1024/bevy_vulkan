use std::sync::Mutex;

use ash::vk;
use bevy::{
    asset::Asset,
    math::{Vec2, Vec3},
    pbr::StandardMaterial,
    reflect::TypePath,
};
use bytemuck::{Pod, Zeroable};
use half::f16;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use crate::{
    render_buffer::{Buffer, BufferProvider},
    render_device::RenderDevice,
    render_texture::RenderTexture,
    vulkan_asset::VulkanAsset,
};

#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
}

#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
pub struct Triangle {
    pub tangent: u32,
    pub normals: [u32; 3],
    pub uvs: [u32; 3],
}

impl Triangle {
    pub fn pack_normal(n: &Vec3) -> u32 {
        let x = (n.x * 0.5 + 0.5) * 65535.0;
        let y = (n.y * 0.5 + 0.5) * 32767.0;
        let z = if n.z >= 0.0 { 0 } else { 1 };
        ((x as u32) << 16) | ((y as u32) << 1) | z
    }

    // inverse of unpackHalf2x16 in glsl
    pub fn pack_uv(uv: &Vec2) -> u32 {
        let x = f16::from_f32(uv.x).to_bits();
        let y = f16::from_f32(uv.y).to_bits();
        ((y as u32) << 16) | (x as u32)
    }
}

#[derive(Debug)]
pub struct GeometryDescr {
    pub first_vertex: usize,
    pub vertex_count: usize,
    pub first_index: usize,
    pub index_count: usize,
}

#[derive(TypePath, Asset, Debug, Clone, Copy)]
#[repr(C)]
pub struct RTXMaterial {
    pub base_color_factor: [f32; 4],
    pub base_emissive_factor: [f32; 4],
    pub base_color_texture: u32,
    pub base_emissive_texture: u32,
    pub specular_transmission_texture: u32,
    pub metallic_roughness_texture: u32,
    pub normal_texture: u32,
    pub specular_transmission_factor: f32,
    pub roughness_factor: f32,
    pub metallic_factor: f32,
    pub refract_index: f32,
}

impl RTXMaterial {
    pub fn from_bevy_standard_material(material: &StandardMaterial) -> Self {
        RTXMaterial {
            base_color_factor: {
                let c = material.base_color.to_linear();
                [c.red, c.green, c.blue, c.alpha]
            },
            base_emissive_factor: {
                let c = material.emissive;
                [c.red, c.green, c.blue, c.alpha]
            },
            base_color_texture: 0xffffffff,
            base_emissive_texture: 0xffffffff,
            normal_texture: 0xffffffff,
            specular_transmission_texture: 0xffffffff,
            metallic_roughness_texture: 0xffffffff,
            specular_transmission_factor: material.specular_transmission,
            roughness_factor: material.perceptual_roughness,
            metallic_factor: material.metallic,
            refract_index: material.ior,
        }
    }
}

impl Default for RTXMaterial {
    fn default() -> Self {
        RTXMaterial {
            base_color_factor: [0.5, 0.5, 0.5, 1.0],
            base_emissive_factor: [0.0, 0.0, 0.0, 0.0],
            base_color_texture: 0xffffffff,
            base_emissive_texture: 0xffffffff,
            normal_texture: 0xffffffff,
            specular_transmission_texture: 0xffffffff,
            metallic_roughness_texture: 0xffffffff,
            specular_transmission_factor: 0.0,
            roughness_factor: 1.0,
            metallic_factor: 0.0,
            refract_index: 1.0,
        }
    }
}

impl VulkanAsset for StandardMaterial {
    type ExtractedAsset = RTXMaterial;
    type ExtractParam = ();
    type PreparedAsset = RTXMaterial;

    fn extract_asset(
        &self,
        _param: &mut bevy::ecs::system::SystemParamItem<Self::ExtractParam>,
    ) -> Option<Self::ExtractedAsset> {
        Some(RTXMaterial::from_bevy_standard_material(self))
    }

    fn prepare_asset(
        asset: Self::ExtractedAsset,
        _render_device: &RenderDevice,
    ) -> Self::PreparedAsset {
        asset
    }

    fn destroy_asset(_render_device: &RenderDevice, _prepared_asset: &Self::PreparedAsset) {}
}

pub struct BLAS {
    pub acceleration_structure: AccelerationStructure,
    pub vertex_buffer: Buffer<Vertex>,
    pub triangle_buffer: Buffer<Triangle>,
    pub index_buffer: Buffer<u32>,
    pub geometry_to_index: Buffer<u32>,
    pub geometry_to_triangle: Buffer<u32>,
    pub gltf_materials: Option<Vec<RTXMaterial>>,
    pub gltf_textures: Option<Vec<RenderTexture>>,
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
            .destroy_buffer(self.triangle_buffer.handle);
        render_device
            .destroyer
            .destroy_buffer(self.index_buffer.handle);
        render_device
            .destroyer
            .destroy_buffer(self.geometry_to_index.handle);
        render_device
            .destroyer
            .destroy_buffer(self.geometry_to_triangle.handle);
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

    pub fn destroy(&self, render_device: &RenderDevice) {
        render_device
            .destroyer
            .destroy_acceleration_structure(self.handle);
        render_device.destroyer.destroy_buffer(self.buffer.handle);
    }
}

pub fn build_blas_from_buffers(
    render_device: &RenderDevice,
    vertex_count: usize,
    index_count: usize,
    mut vertex_buffer_host: Buffer<Vertex>,
    mut index_buffer_host: Buffer<u32>,
    geometries: &[GeometryDescr],
) -> BLAS {
    log::info!(
        "Building BLAS for mesh with {} vertices and {} indices and {} geometries",
        vertex_count,
        index_count,
        geometries.len()
    );

    let vertex_buffer = render_device.map_buffer(&mut vertex_buffer_host);
    let index_buffer = render_device.map_buffer(&mut index_buffer_host);

    let mut geom_to_index_host: Buffer<u32> = render_device.create_host_buffer(
        geometries.len() as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
    );
    let mut geom_to_index = render_device.map_buffer(&mut geom_to_index_host);
    for (i, geometry) in geometries.iter().enumerate() {
        geom_to_index[i] = geometry.first_index as u32;
    }

    let mut geom_to_triangle_index_host: Buffer<u32> = render_device.create_host_buffer(
        geometries.len() as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
    );
    let mut geom_to_triangle = render_device.map_buffer(&mut geom_to_triangle_index_host);

    let mut triangle_buffer_host: Buffer<Triangle> = render_device.create_host_buffer(
        index_count as u64 / 3,
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
    );

    let mut prefix_sum = 0;
    for (i, geometry) in geometries.iter().enumerate() {
        geom_to_triangle[i] = prefix_sum;
        prefix_sum += geometry.index_count as u32 / 3;
    }

    let triangle_buffer = Mutex::new(render_device.map_buffer(&mut triangle_buffer_host));
    let work = geometries
        .iter()
        .zip(geom_to_triangle.as_slice_mut().iter().copied())
        .enumerate()
        .collect::<Vec<_>>();

    work.into_par_iter().for_each(|(gi, (geometry, offset))| {
        let mut buffer = vec![Triangle::default(); geometry.index_count / 3];
        for tid in 0..(geometry.index_count / 3) {
            let v0 = vertex_buffer[index_buffer[geometry.first_index + tid * 3 + 0] as usize];
            let v1 = vertex_buffer[index_buffer[geometry.first_index + tid * 3 + 1] as usize];
            let v2 = vertex_buffer[index_buffer[geometry.first_index + tid * 3 + 2] as usize];

            let edge1 = v1.position - v0.position;
            let edge2 = v2.position - v0.position;
            let delta_uv1 = v1.uv - v0.uv;
            let delta_uv2 = v2.uv - v0.uv;

            let denom = delta_uv1.x * delta_uv2.y - delta_uv1.y * delta_uv2.x;
            let tangent = if denom.abs() < 0.0001 {
                Vec3::Z
            } else {
                let f = 1.0 / denom;
                Vec3::new(
                    f * (delta_uv2.y * edge1.x - delta_uv1.y * edge2.x),
                    f * (delta_uv2.y * edge1.y - delta_uv1.y * edge2.y),
                    f * (delta_uv2.y * edge1.z - delta_uv1.y * edge2.z),
                )
                .normalize()
            };
            buffer[tid] = Triangle {
                tangent: Triangle::pack_normal(&tangent),
                normals: [
                    Triangle::pack_normal(&v0.normal),
                    Triangle::pack_normal(&v1.normal),
                    Triangle::pack_normal(&v2.normal),
                ],
                uvs: [
                    Triangle::pack_uv(&v0.uv),
                    Triangle::pack_uv(&v1.uv),
                    Triangle::pack_uv(&v2.uv),
                ],
            };
        }
        log::info!(
            "Packed geometry {}/{} with {} triangles",
            gi,
            geometries.len(),
            geometry.index_count / 3
        );

        let mut triangle_buffer = triangle_buffer.lock().unwrap();
        for (i, t) in buffer.iter().enumerate() {
            triangle_buffer[offset as usize + i] = *t;
        }
    });

    let vertex_buffer_device: Buffer<Vertex> = render_device.create_device_buffer(
        vertex_count as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
    );

    let index_buffer_device: Buffer<u32> = render_device.create_device_buffer(
        index_count as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
    );

    let triangle_buffer_device: Buffer<Triangle> = render_device.create_device_buffer(
        index_count as u64 / 3,
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
    );

    let geom_to_index_device: Buffer<u32> = render_device.create_device_buffer(
        geometries.len() as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
    );

    let geom_to_triangle_device: Buffer<u32> = render_device.create_device_buffer(
        geometries.len() as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
    );

    render_device.run_transfer_commands(|cmd_buffer| {
        render_device.upload_buffer(cmd_buffer, &vertex_buffer_host, &vertex_buffer_device);
        render_device.upload_buffer(cmd_buffer, &index_buffer_host, &index_buffer_device);
        render_device.upload_buffer(cmd_buffer, &triangle_buffer_host, &triangle_buffer_device);
        render_device.upload_buffer(cmd_buffer, &geom_to_index_host, &geom_to_index_device);
        render_device.upload_buffer(
            cmd_buffer,
            &geom_to_triangle_index_host,
            &geom_to_triangle_device,
        );
    });

    render_device
        .destroyer
        .destroy_buffer(vertex_buffer_host.handle);
    render_device
        .destroyer
        .destroy_buffer(triangle_buffer_host.handle);
    render_device
        .destroyer
        .destroy_buffer(index_buffer_host.handle);
    render_device
        .destroyer
        .destroy_buffer(geom_to_index_host.handle);
    render_device
        .destroyer
        .destroy_buffer(geom_to_triangle_index_host.handle);

    let geometry_infos = geometries
        .iter()
        .map(|_| {
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
                        .max_vertex(vertex_count as u32)
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
        .flags(
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
                | vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION,
        )
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

    render_device.run_transfer_commands(&|cmd_buffer| unsafe {
        render_device
            .ext_acc_struct
            .cmd_build_acceleration_structures(
                cmd_buffer,
                std::slice::from_ref(&build_geometry_info),
                singleton_build_ranges,
            )
    });

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

    // compaction
    let query_pool_info = vk::QueryPoolCreateInfo::default()
        .query_type(vk::QueryType::ACCELERATION_STRUCTURE_COMPACTED_SIZE_KHR)
        .query_count(1);

    let query_pool = unsafe {
        render_device
            .device
            .create_query_pool(&query_pool_info, None)
    }
    .unwrap();
    unsafe {
        render_device.run_transfer_commands(&|cmd_buffer| {
            render_device
                .device
                .cmd_reset_query_pool(cmd_buffer, query_pool, 0, 1);
        })
    }

    unsafe {
        render_device.run_transfer_commands(&|cmd_buffer| {
            render_device
                .ext_acc_struct
                .cmd_write_acceleration_structures_properties(
                    cmd_buffer,
                    std::slice::from_ref(&acceleration_structure.handle),
                    vk::QueryType::ACCELERATION_STRUCTURE_COMPACTED_SIZE_KHR,
                    query_pool,
                    0,
                );
        })
    }

    let mut compacted_sizes = [0];
    unsafe {
        render_device
            .device
            .get_query_pool_results::<u64>(
                query_pool,
                0,
                &mut compacted_sizes,
                vk::QueryResultFlags::WAIT,
            )
            .unwrap();
    };

    log::info!(
        "BLAS compaction: {} -> {} ({}%)",
        size_info.acceleration_structure_size,
        compacted_sizes[0],
        (compacted_sizes[0] as f32 / size_info.acceleration_structure_size as f32) * 100.0
    );

    let compacted_buffer = render_device.create_device_buffer::<u8>(
        compacted_sizes[0],
        vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR,
    );

    let compacted_as_info = vk::AccelerationStructureCreateInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
        .size(compacted_sizes[0])
        .buffer(compacted_buffer.handle);

    let compacted_as = unsafe {
        render_device
            .ext_acc_struct
            .create_acceleration_structure(&compacted_as_info, None)
    }
    .unwrap();

    unsafe {
        render_device.run_transfer_commands(&|cmd_buffer| {
            let copy_info = vk::CopyAccelerationStructureInfoKHR::default()
                .src(acceleration_structure.handle)
                .dst(compacted_as)
                .mode(vk::CopyAccelerationStructureModeKHR::COMPACT);
            render_device
                .ext_acc_struct
                .cmd_copy_acceleration_structure(cmd_buffer, &copy_info);
        })
    }

    unsafe {
        render_device
            .destroyer
            .destroy_acceleration_structure(acceleration_structure.handle);
        render_device
            .destroyer
            .destroy_buffer(acceleration_structure.buffer.handle);
        render_device.device.destroy_query_pool(query_pool, None);
    }
    acceleration_structure.buffer = compacted_buffer;
    acceleration_structure.handle = compacted_as;
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
        triangle_buffer: triangle_buffer_device,
        index_buffer: index_buffer_device,
        geometry_to_index: geom_to_index_device,
        geometry_to_triangle: geom_to_triangle_device,
        gltf_materials: None,
        gltf_textures: None,
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
