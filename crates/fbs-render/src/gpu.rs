use std::sync::Arc;

/// Human-readable summary of what is actually doing the drawing. On the 840 G2
/// this should read Intel HD Graphics 5500; if it says llvmpipe or "Microsoft
/// Basic Render Driver", the Intel driver is missing and compositing will be slow.
#[derive(Debug, Clone)]
pub struct AdapterReport {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    pub max_texture_dim: u32,
}

impl AdapterReport {
    pub fn is_software(&self) -> bool {
        let n = self.name.to_ascii_lowercase();
        matches!(self.device_type.as_str(), "Cpu" | "VirtualGpu" | "Other")
            || n.contains("llvmpipe")
            || n.contains("basic render")
            || n.contains("software")
    }

    /// One line for a status bar.
    pub fn headline(&self) -> String {
        let soft = if self.is_software() { " (software)" } else { "" };
        format!("{}{} · {}", self.name, soft, self.backend)
    }
}

/// Owns the logical device.
pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub report: AdapterReport,
}

impl Gpu {
    /// Create from scratch. Prefers a real GPU but accepts a software adapter so
    /// the app still runs (slowly) on a machine with no working driver, instead
    /// of refusing to start.
    pub fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: backends_from_env(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .or_else(|| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: true,
            }))
        })
        .ok_or_else(|| "no usable GPU adapter was found".to_string())?;

        Self::with_adapter(adapter)
    }

    pub fn with_adapter(adapter: wgpu::Adapter) -> Result<Self, String> {
        let info = adapter.get_info();
        let limits = adapter.limits();
        let report = AdapterReport {
            name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            driver: info.driver_info.clone(),
            max_texture_dim: limits.max_texture_dimension_2d,
        };

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("fbs-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(limits.clone()),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .map_err(|e| format!("device creation failed: {e}"))?;

        Ok(Self {
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            report,
        })
    }

    /// Upload an RGBA8 image as a sampled texture.
    pub fn texture_from_rgba(&self, w: u32, h: u32, pixels: &[u8]) -> wgpu::Texture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fbs-upload"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        texture
    }
}

/// Let `WGPU_BACKEND=vulkan|dx12|gl` override detection, which is invaluable when
/// chasing a driver problem on a specific laptop.
fn backends_from_env() -> wgpu::Backends {
    match std::env::var("WGPU_BACKEND").ok().as_deref() {
        Some("vulkan") => wgpu::Backends::VULKAN,
        Some("dx12") => wgpu::Backends::DX12,
        Some("dx11") => wgpu::Backends::DX12,
        Some("gl") => wgpu::Backends::GL,
        Some("metal") => wgpu::Backends::METAL,
        _ => wgpu::Backends::all(),
    }
}
