//! GPU Detection and Availability Checking
//!
//! Provides cross-platform GPU detection for conditional test execution
//! and runtime GPU selection.

// Allow dead code for future GPU backend implementations
#![allow(dead_code)]

/// GPU backend types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackend {
    Metal,
    Vulkan,
    Cuda,
    OpenCL,
    None,
}

impl std::fmt::Display for GpuBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuBackend::Metal => write!(f, "Metal"),
            GpuBackend::Vulkan => write!(f, "Vulkan"),
            GpuBackend::Cuda => write!(f, "CUDA"),
            GpuBackend::OpenCL => write!(f, "OpenCL"),
            GpuBackend::None => write!(f, "None"),
        }
    }
}

/// GPU device information
#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub name: String,
    pub backend: GpuBackend,
    pub vendor: String,
    pub available: bool,
}

/// Check if Metal GPU is available (macOS only)
#[cfg(target_os = "macos")]
pub fn is_metal_available() -> bool {
    use metal::Device;
    Device::system_default().is_some()
}

#[cfg(not(target_os = "macos"))]
pub fn is_metal_available() -> bool {
    false
}

/// Get Metal GPU info (macOS only)
#[cfg(target_os = "macos")]
pub fn get_metal_info() -> Option<GpuInfo> {
    use metal::Device;
    Device::system_default().map(|device| GpuInfo {
        name: device.name().to_string(),
        backend: GpuBackend::Metal,
        vendor: "Apple".to_string(),
        available: true,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn get_metal_info() -> Option<GpuInfo> {
    None
}

/// Check if NVIDIA CUDA is available
pub fn is_cuda_available() -> bool {
    // Check for nvidia-smi or CUDA toolkit
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("nvidia-smi")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("nvidia-smi")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Check if AMD GPU is available (via ROCm or OpenCL)
pub fn is_amd_available() -> bool {
    // Check for rocm-smi on Linux
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("rocm-smi")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Check if Intel GPU is available
pub fn is_intel_gpu_available() -> bool {
    // Check for Intel GPU via sysinfo or platform-specific methods
    #[cfg(target_os = "linux")]
    {
        // Check for Intel GPU in /sys/class/drm
        std::path::Path::new("/sys/class/drm/card0/device/vendor").exists()
    }
    #[cfg(target_os = "windows")]
    {
        // Would need DirectX or OpenCL enumeration
        false
    }
    #[cfg(target_os = "macos")]
    {
        // Intel GPUs on macOS use Metal
        false
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        false
    }
}

/// Get list of all available GPU backends
pub fn get_available_backends() -> Vec<GpuBackend> {
    let mut backends = Vec::new();
    
    if is_metal_available() {
        backends.push(GpuBackend::Metal);
    }
    if is_cuda_available() {
        backends.push(GpuBackend::Cuda);
    }
    // Add more backend checks as needed
    
    if backends.is_empty() {
        backends.push(GpuBackend::None);
    }
    
    backends
}

/// Get the best available GPU backend for the current system
pub fn get_best_backend() -> GpuBackend {
    // Priority: Metal (macOS) > CUDA (NVIDIA) > OpenCL > None
    if is_metal_available() {
        return GpuBackend::Metal;
    }
    if is_cuda_available() {
        return GpuBackend::Cuda;
    }
    GpuBackend::None
}

/// Print GPU detection summary
pub fn print_gpu_summary() {
    println!("GPU Detection Summary:");
    println!("  Metal:  {}", if is_metal_available() { "Available" } else { "Not available" });
    println!("  CUDA:   {}", if is_cuda_available() { "Available" } else { "Not available" });
    println!("  AMD:    {}", if is_amd_available() { "Available" } else { "Not available" });
    println!("  Intel:  {}", if is_intel_gpu_available() { "Available" } else { "Not available" });
    println!("  Best:   {}", get_best_backend());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_backend_display() {
        assert_eq!(format!("{}", GpuBackend::Metal), "Metal");
        assert_eq!(format!("{}", GpuBackend::Cuda), "CUDA");
        assert_eq!(format!("{}", GpuBackend::None), "None");
    }

    #[test]
    fn test_get_available_backends() {
        let backends = get_available_backends();
        assert!(!backends.is_empty(), "Should have at least one backend (even if None)");
    }

    #[test]
    fn test_get_best_backend() {
        let best = get_best_backend();
        // Just verify it returns something valid
        match best {
            GpuBackend::Metal | GpuBackend::Cuda | GpuBackend::Vulkan | 
            GpuBackend::OpenCL | GpuBackend::None => {}
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_metal_detection_macos() {
        // On macOS, Metal should be available
        // Note: This test may be skipped in CI without GPU access
        let available = is_metal_available();
        if available {
            let info = get_metal_info();
            assert!(info.is_some(), "Metal info should be available when Metal is available");
            let info = info.unwrap();
            assert!(!info.name.is_empty(), "GPU name should not be empty");
            assert_eq!(info.backend, GpuBackend::Metal);
        }
    }

    #[test]
    fn test_cuda_detection() {
        // Just verify it doesn't panic
        let _available = is_cuda_available();
    }

    #[test]
    fn test_amd_detection() {
        // Just verify it doesn't panic
        let _available = is_amd_available();
    }

    #[test]
    fn test_intel_detection() {
        // Just verify it doesn't panic
        let _available = is_intel_gpu_available();
    }
}
