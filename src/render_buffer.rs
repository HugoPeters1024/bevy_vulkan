use ash::vk;
use gpu_allocator::{
    vulkan::{AllocationCreateDesc, AllocationScheme},
    MemoryLocation,
};

use crate::render_device::RenderDevice;

#[derive(Debug)]
pub struct Buffer<T> {
    pub nr_elements: u64,
    pub handle: vk::Buffer,
    pub address: u64,
    marker: std::marker::PhantomData<T>,
}

impl<T> Default for Buffer<T> {
    fn default() -> Self {
        Buffer {
            nr_elements: 0,
            handle: vk::Buffer::null(),
            address: 0,
            marker: std::marker::PhantomData,
        }
    }
}

pub struct BufferView<T> {
    pub nr_elements: u64,
    ptr: *mut T,
    marker: std::marker::PhantomData<T>,
}


impl<T> BufferView<T> {
    pub fn as_slice_mut(&mut self) -> &mut [T] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.nr_elements as usize) }
    }

    pub fn as_ptr_mut(&mut self) -> *mut T {
        self.ptr
    }

    pub fn copy_from_slice(&mut self, slice: &[T]) {
        let len = std::cmp::min(slice.len(), self.nr_elements as usize);
        unsafe {
            std::ptr::copy_nonoverlapping(slice.as_ptr(), self.ptr, len);
        }
    }
}

unsafe impl<T: Send> Send for BufferView<T> {}
unsafe impl<T: Sync> Sync for BufferView<T> {}

impl<'a, T> std::ops::Index<usize> for BufferView<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        unsafe { self.ptr.add(index).as_ref().unwrap() }
    }
}

impl<'a, T> std::ops::IndexMut<usize> for BufferView<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        unsafe { self.ptr.add(index).as_mut().unwrap() }
    }
}

pub trait BufferProvider {
    fn create_host_buffer<T>(&self, size: u64, usage: vk::BufferUsageFlags) -> Buffer<T>;

    fn create_device_buffer<T>(&self, size: u64, usage: vk::BufferUsageFlags) -> Buffer<T>;

    fn create_buffer<T>(
        &self,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
    ) -> Buffer<T>;

    fn upload_buffer<T>(
        &self,
        cmd_buffer: vk::CommandBuffer,
        host_buffer: &Buffer<T>,
        device_buffer: &Buffer<T>,
    );

    fn map_buffer<T>(&self, buffer: &mut Buffer<T>) -> BufferView<T>;
}

impl BufferProvider for RenderDevice {
    fn create_host_buffer<T>(&self, size: u64, usage: vk::BufferUsageFlags) -> Buffer<T> {
        self.create_buffer(
            size,
            usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            MemoryLocation::CpuToGpu,
        )
    }

    fn create_device_buffer<T>(&self, size: u64, usage: vk::BufferUsageFlags) -> Buffer<T> {
        self.create_buffer(
            size,
            usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            MemoryLocation::GpuOnly,
        )
    }

    fn create_buffer<T>(
        &self,
        nr_elements: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
    ) -> Buffer<T> {
        if nr_elements == 0 {
            return Buffer {
                nr_elements,
                handle: vk::Buffer::null(),
                address: 0,
                marker: std::marker::PhantomData,
            };
        }
        let buffer_info = vk::BufferCreateInfo::default()
            .size(nr_elements * std::mem::size_of::<T>() as u64)
            .usage(usage);

        let handle = unsafe { self.device.create_buffer(&buffer_info, None).unwrap() };
        let requirements = unsafe { self.device.get_buffer_memory_requirements(handle) };

        {
            let mut state = self.allocator_state.write().unwrap();
            let allocation = state
                .allocate(&AllocationCreateDesc {
                    name: "Buffer Allocation",
                    requirements,
                    location,
                    linear: true,
                    allocation_scheme: AllocationScheme::DedicatedBuffer(handle),
                })
                .unwrap();

            unsafe {
                self.bind_buffer_memory(handle, allocation.memory(), allocation.offset())
                    .unwrap();
            }

            state.register_buffer_allocation(handle, allocation);
        }

        let address = unsafe {
            self.get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(handle))
        };

        Buffer {
            handle,
            nr_elements,
            address,
            marker: std::marker::PhantomData,
        }
    }

    fn upload_buffer<T>(
        &self,
        cmd_buffer: vk::CommandBuffer,
        host_buffer: &Buffer<T>,
        device_buffer: &Buffer<T>,
    ) {
        unsafe {
            let copy_region = vk::BufferCopy::default()
                .src_offset(0)
                .dst_offset(0)
                .size(host_buffer.nr_elements * std::mem::size_of::<T>() as u64);
            self.cmd_copy_buffer(
                cmd_buffer,
                host_buffer.handle,
                device_buffer.handle,
                &[copy_region],
            );
        }
    }

    fn map_buffer<T>(&self, buffer: &mut Buffer<T>) -> BufferView<T> {
        let state = self.allocator_state.read().unwrap();
        let ptr = state
            .get_buffer_allocation(buffer.handle)
            .unwrap()
            .mapped_ptr()
            .unwrap()
            .as_ptr()
            .cast::<T>();

        BufferView {
            nr_elements: buffer.nr_elements,
            ptr,
            marker: std::marker::PhantomData,
        }
    }
}

impl<T> Drop for Buffer<T> {
    fn drop(&mut self) {}
}
