use crate::pond::Pond;
use wgpu::util::DeviceExt;

const MAX_SPLASHES: usize = 64;
const MAX_FISH: usize = 16;
const MAX_FOOD: usize = 256;

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    wave: wgpu::ComputePipeline,
    shade: wgpu::ComputePipeline,
    bind_groups: [wgpu::BindGroup; 2],
    current: usize,
    params: wgpu::Buffer,
    splashes: wgpu::Buffer,
    fish: wgpu::Buffer,
    food: wgpu::Buffer,
    out: wgpu::Buffer,
    readback: wgpu::Buffer,
    w: u32,
    h: u32,
    rgb: bool,
    pub adapter: String,
}

impl Gpu {
    pub fn new(w: usize, h: usize, rgb: bool) -> Gpu {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })).expect("no Vulkan adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits { max_storage_buffer_binding_size: adapter.limits().max_storage_buffer_binding_size, max_buffer_size: adapter.limits().max_buffer_size, ..Default::default() },
            ..Default::default()
        }))
        .expect("no device");

        let (w, h) = (u32::try_from(w).unwrap(), u32::try_from(h).unwrap());
        let pixels = u64::from(w) * u64::from(h);
        let out_size = if rgb { pixels * 3 } else { pixels * 4 };
        let storage = |label, size, usage| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::STORAGE | usage, mapped_at_creation: false });
        let heights = [storage("height a", pixels * 4, wgpu::BufferUsages::COPY_DST), storage("height b", pixels * 4, wgpu::BufferUsages::COPY_DST)];
        let splashes = storage("splashes", 16 * MAX_SPLASHES as u64, wgpu::BufferUsages::COPY_DST);
        let fish = storage("fish", 64 * MAX_FISH as u64, wgpu::BufferUsages::COPY_DST);
        let food = storage("food", 16 * MAX_FOOD as u64, wgpu::BufferUsages::COPY_DST);
        let out = storage("out", out_size, wgpu::BufferUsages::COPY_SRC);
        let readback = device.create_buffer(&wgpu::BufferDescriptor { label: Some("readback"), size: out_size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("params"), contents: &[0u8; 32], usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST });

        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let read = wgpu::BufferBindingType::Storage { read_only: true };
        let write = wgpu::BufferBindingType::Storage { read_only: false };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[entry(0, wgpu::BufferBindingType::Uniform), entry(1, read), entry(2, write), entry(3, read), entry(4, read), entry(5, read), entry(6, write)],
        });
        let bind_groups = [0, 1].map(|k| {
            let buffers = [&params, &heights[k], &heights[1 - k], &splashes, &fish, &food, &out];
            let entries: Vec<wgpu::BindGroupEntry> = buffers.iter().enumerate().map(|(n, b)| wgpu::BindGroupEntry { binding: u32::try_from(n).unwrap(), resource: b.as_entire_binding() }).collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &layout, entries: &entries })
        });

        let module = device.create_shader_module(wgpu::include_wgsl!("pond.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = |entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let (wave, shade) = (pipeline("wave"), pipeline("shade"));

        Gpu { adapter: adapter.get_info().name, device, queue, wave, shade, bind_groups, current: 0, params, splashes, fish, food, out, readback, w, h, rgb }
    }

    fn write_params(&self, splashes: usize, fish: usize, food: usize) {
        let counts = [splashes, fish, food].map(|n| u32::try_from(n).unwrap());
        let params: [u32; 8] = [self.w, self.h, counts[0], counts[1], counts[2], u32::from(self.rgb), 0, 0];
        self.queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&params));
    }

    pub fn step(&mut self, splashes: &[[f32; 4]]) {
        let splashes = &splashes[..splashes.len().min(MAX_SPLASHES)];
        self.write_params(splashes.len(), 0, 0);
        if !splashes.is_empty() {
            self.queue.write_buffer(&self.splashes, 0, bytemuck::cast_slice(splashes));
        }
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.wave);
            pass.set_bind_group(0, &self.bind_groups[self.current], &[]);
            pass.dispatch_workgroups(self.w.div_ceil(16), self.h.div_ceil(16), 1);
        }
        self.queue.submit([encoder.finish()]);
        self.current = 1 - self.current;
    }

    /// Shades the current state, waits for the readback, and hands the finished frame to `sink`.
    pub fn render<R>(&mut self, pond: &Pond, sink: impl FnOnce(&[u8]) -> R) -> R {
        let fish: Vec<[f32; 16]> = pond
            .fish
            .iter()
            .take(MAX_FISH)
            .map(|f| {
                let speed = (f.vx * f.vx + f.vy * f.vy).sqrt().max(0.001);
                let [br, bg, bb] = f.base;
                let [sr, sg, sb] = f.spot;
                [f.x, f.y, f.vx / speed, f.vy / speed, f.len, f.phase, f.seed, f.spot_threshold, br, bg, bb, 0.0, sr, sg, sb, 0.0]
            })
            .collect();
        let food: Vec<[f32; 4]> = pond.food.iter().take(MAX_FOOD).map(|f| [f.x, f.y, f.age, 0.0]).collect();
        self.write_params(0, fish.len(), food.len());
        if !fish.is_empty() {
            self.queue.write_buffer(&self.fish, 0, bytemuck::cast_slice(&fish));
        }
        if !food.is_empty() {
            self.queue.write_buffer(&self.food, 0, bytemuck::cast_slice(&food));
        }

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.shade);
            pass.set_bind_group(0, &self.bind_groups[self.current], &[]);
            pass.dispatch_workgroups((self.w * self.h / 4).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&self.out, 0, &self.readback, 0, None);
        self.queue.submit([encoder.finish()]);
        self.readback.map_async(wgpu::MapMode::Read, .., |result| result.expect("readback map failed"));
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("gpu poll failed");
        let result = sink(&self.readback.get_mapped_range(..).expect("readback not mapped"));
        self.readback.unmap();
        result
    }
}
