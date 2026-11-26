//! Reports which GPU wgpu will use, and whether it is hardware.
//!
//! Run this on the laptop before blaming the compositor for being slow:
//!   cargo run -p fbs-render --example gpu_report
//!
//! Expected on the 840 G2: "Intel(R) HD Graphics 5500" on Vulkan or DX12.
//! If it says llvmpipe or "Microsoft Basic Render Driver", Windows is using a
//! fallback driver and hardware capture/compositing will not work properly.

use fbs_render::Gpu;

fn main() {
    match Gpu::new() {
        Ok(gpu) => {
            let r = &gpu.report;
            println!("adapter     : {}", r.name);
            println!("backend     : {}", r.backend);
            println!("device type : {}", r.device_type);
            println!("driver      : {}", r.driver);
            println!("max texture : {} px", r.max_texture_dim);
            println!();
            if r.is_software() {
                println!("VERDICT: software rendering.");
                println!("Install the real Intel HD 5500 driver, then run this again.");
            } else {
                println!("VERDICT: hardware GPU. Compositing and capture should behave.");
            }
        }
        Err(e) => {
            eprintln!("no GPU available: {e}");
            std::process::exit(1);
        }
    }
}
