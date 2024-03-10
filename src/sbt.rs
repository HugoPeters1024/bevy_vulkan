use crate::{
    gltf_mesh::Gltf,
    ray_render_plugin::{Render, RenderConfig, RenderSet, TeardownSchedule},
    raytracing_pipeline::{RTGroupHandle, RaytracingPipeline},
    render_buffer::{Buffer, BufferProvider},
    render_device::RenderDevice,
    tlas_builder::{update_tlas, TLAS},
    vk_utils,
    vulkan_asset::{poll_for_asset, VulkanAssets},
};
use ash::vk;
use bevy::{prelude::*, render::RenderApp};

#[derive(Clone, Copy)]
#[repr(C)]
pub struct SBTRegionRaygen {
    pub handle: RTGroupHandle,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct SBTRegionMiss {
    pub handle: RTGroupHandle,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SBTRegionHitTriangle {
    pub handle: RTGroupHandle,
    pub vertex_buffer: vk::DeviceAddress,
    pub index_buffer: vk::DeviceAddress,
    pub geometry_to_index: [u32; 32],
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SBTRegionHitSphere {
    pub handle: RTGroupHandle,
}

#[derive(Default)]
pub struct SBTAligments {
    initialized: bool,
    shader_group_base_alignment: u64,
    shader_group_handle_alignment: u64,
}

#[derive(Default, Resource)]
pub struct SBT {
    pub raygen_region: vk::StridedDeviceAddressRegionKHR,
    pub miss_region: vk::StridedDeviceAddressRegionKHR,
    pub hit_region: vk::StridedDeviceAddressRegionKHR,
    pub data: Buffer<u8>,
}

fn update_sbt(
    render_device: Res<RenderDevice>,
    mut sbt: ResMut<SBT>,
    tlas: Res<TLAS>,
    rtx_pipelines: Res<VulkanAssets<RaytracingPipeline>>,
    meshes: Res<VulkanAssets<Mesh>>,
    gltf_meshes: Res<VulkanAssets<Gltf>>,
    render_config: Res<RenderConfig>,
    mut aligments: Local<SBTAligments>,
) {
    if !aligments.initialized {
        let rtprops = vk_utils::get_raytracing_properties(&render_device);
        aligments.shader_group_base_alignment = rtprops.shader_group_base_alignment as u64;
        aligments.shader_group_handle_alignment = rtprops.shader_group_handle_alignment as u64;
        aligments.initialized = true;
    }
    let Some(rtx_pipeline) = rtx_pipelines.get(&render_config.rtx_pipeline) else {
        return;
    };

    let handle_size_aligned = vk_utils::aligned_size(
        std::mem::size_of::<RTGroupHandle>() as u64,
        aligments.shader_group_handle_alignment,
    );

    sbt.raygen_region.stride =
        vk_utils::aligned_size(handle_size_aligned, aligments.shader_group_base_alignment);
    sbt.raygen_region.size = sbt.raygen_region.stride;

    sbt.miss_region.stride =
        vk_utils::aligned_size(handle_size_aligned, aligments.shader_group_base_alignment);
    sbt.miss_region.size = sbt.miss_region.stride;

    sbt.hit_region.stride = vk_utils::aligned_size(
        [
            std::mem::size_of::<SBTRegionHitTriangle>(),
            std::mem::size_of::<SBTRegionHitSphere>(),
        ]
        .into_iter()
        .max()
        .unwrap() as u64,
        aligments.shader_group_base_alignment,
    );

    // one extra for the sphere hit group
    sbt.hit_region.size = sbt.hit_region.stride * (meshes.len() + 1) as u64;

    let total_size = sbt.raygen_region.size + sbt.miss_region.size + sbt.hit_region.size;

    // recreate the buffer if the size has changed
    if sbt.data.nr_elements != total_size {
        render_device.destroyer.destroy_buffer(sbt.data.handle);
        sbt.data = render_device
            .create_host_buffer(total_size, vk::BufferUsageFlags::SHADER_BINDING_TABLE_KHR);
    }

    {
        let mut data = render_device.map_buffer(&mut sbt.data);
        unsafe {
            let mut dst: *mut u8 = data.as_ptr_mut();

            // raygen region (only a handle)
            (dst as *mut SBTRegionRaygen).write(SBTRegionRaygen {
                handle: rtx_pipeline.raygen_handle,
            });
            dst = dst.add(sbt.raygen_region.size as usize);

            // miss region (comes after the raygen region)
            (dst as *mut SBTRegionMiss).write(SBTRegionMiss {
                handle: rtx_pipeline.miss_handle,
            });
            dst = dst.add(sbt.miss_region.size as usize);

            // hit regions (come after the miss region)
            (dst as *mut SBTRegionHitSphere).write(SBTRegionHitSphere {
                handle: rtx_pipeline.sphere_hit_handle,
            });

            for (mesh_id, mesh) in meshes.iter() {
                let mut geometry_to_index = [0; 32];
                for (i, index) in mesh.geometry_to_index.iter().enumerate() {
                    geometry_to_index[i] = *index;
                }

                let offset = tlas.mesh_to_hit_offset[&mesh_id.untyped()];
                (dst.add(offset as usize * sbt.hit_region.stride as usize)
                    as *mut SBTRegionHitTriangle)
                    .write(SBTRegionHitTriangle {
                        handle: rtx_pipeline.hit_handle,
                        vertex_buffer: mesh.vertex_buffer.address,
                        index_buffer: mesh.index_buffer.address,
                        geometry_to_index,
                    });
            }

            for (mesh_id, mesh) in gltf_meshes.iter() {
                let mut geometry_to_index = [0; 32];
                for (i, index) in mesh.geometry_to_index.iter().enumerate() {
                    geometry_to_index[i] = *index;
                }

                if let Some(offset) = tlas.mesh_to_hit_offset.get(&mesh_id.untyped()) {
                    (dst.add(*offset as usize * sbt.hit_region.stride as usize)
                        as *mut SBTRegionHitTriangle)
                        .write(SBTRegionHitTriangle {
                            handle: rtx_pipeline.hit_handle,
                            vertex_buffer: mesh.vertex_buffer.address,
                            index_buffer: mesh.index_buffer.address,
                            geometry_to_index,
                        });
                }
            }
        }
    }

    sbt.raygen_region.device_address = sbt.data.address;
    sbt.miss_region.device_address = sbt.data.address + sbt.raygen_region.size;
    sbt.hit_region.device_address =
        sbt.data.address + sbt.raygen_region.size + sbt.miss_region.size;
}

fn cleanup_sbt(sbt: Res<SBT>, render_device: Res<RenderDevice>) {
    render_device.destroyer.destroy_buffer(sbt.data.handle);
}

pub struct SBTPlugin;

impl Plugin for SBTPlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app.init_resource::<SBT>();
        render_app.add_systems(
            Render,
            update_sbt
                .in_set(RenderSet::Prepare)
                .after(poll_for_asset::<RaytracingPipeline>)
                .after(update_tlas),
        );
        render_app.add_systems(TeardownSchedule, cleanup_sbt);
    }
}
