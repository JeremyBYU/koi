/// A headless GPU device. Water and koi each build their own pipelines on it. Cloning is
/// cheap and shares the device.
#[derive(Clone)]
pub struct Gpu {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    /// The adapter's name, for the stats line.
    pub adapter: String,
}

impl Gpu {
    /// Opens the most powerful adapter: Vulkan, Metal on macOS, or DX12 on Windows. The error
    /// says why there is none, and the caller falls back to the CPU renderers.
    pub fn new() -> Result<Gpu, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN | wgpu::Backends::METAL | wgpu::Backends::DX12, ..wgpu::InstanceDescriptor::new_without_display_handle() });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() }))
            .map_err(|e| format!("no GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits { max_storage_buffer_binding_size: adapter.limits().max_storage_buffer_binding_size, max_buffer_size: adapter.limits().max_buffer_size, ..Default::default() },
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU device: {e}"))?;
        Ok(Gpu { adapter: adapter.get_info().name, device, queue })
    }

    /// Submits `encoder` after copying the first `out.len()` bytes of `src` into `readback`,
    /// waits for the GPU, and copies them into `out`.
    pub(crate) fn finish_into(&self, mut encoder: wgpu::CommandEncoder, src: &wgpu::Buffer, readback: &wgpu::Buffer, out: &mut [u8]) {
        let len = u64::try_from(out.len()).expect("readback length fits u64");
        encoder.copy_buffer_to_buffer(src, 0, readback, 0, Some(len));
        self.queue.submit([encoder.finish()]);
        readback.map_async(wgpu::MapMode::Read, ..len, |result| result.expect("readback map failed"));
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("gpu poll failed");
        out.copy_from_slice(&readback.get_mapped_range(..len).expect("readback not mapped"));
        readback.unmap();
    }
}
