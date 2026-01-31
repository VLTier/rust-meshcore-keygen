//! GPU Detection and Availability Checking
//!
//! Provides cross-platform GPU detection for conditional test execution
//! and runtime GPU selection.
//!
//! **Priority Order (native first, OpenCL as fallback):**
//! 1. Metal (macOS native - best performance on Apple Silicon)
//! 2. CUDA (NVIDIA native - best performance on NVIDIA GPUs)
//! 3. Vulkan (cross-platform native via wgpu)
//! 4. OpenCL (universal fallback - works on most GPUs)
//! 5. None (CPU only)

// Allow dead code for future GPU backend implementations
#![allow(dead_code)]

/// GPU backend types ordered by preference (native APIs first)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GpuBackend {
    /// Apple Metal - native macOS/iOS GPU API (highest priority on Apple)
    Metal = 0,
    /// NVIDIA CUDA - native NVIDIA GPU API (highest priority on NVIDIA)
    Cuda = 1,
    /// Vulkan - cross-platform native GPU API via wgpu
    Vulkan = 2,
    /// OpenCL - universal fallback (works on most GPUs but slower)
    OpenCL = 3,
    /// No GPU available - CPU only
    None = 99,
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

/// Check if Vulkan is available (cross-platform via wgpu)
pub fn is_vulkan_available() -> bool {
    // Check for vulkaninfo or Vulkan libraries
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("vulkaninfo")
            .arg("--summary")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "windows")]
    {
        // Check if vulkan-1.dll exists
        std::path::Path::new("C:\\Windows\\System32\\vulkan-1.dll").exists()
    }
    #[cfg(target_os = "macos")]
    {
        // MoltenVK provides Vulkan on macOS, but Metal is preferred
        false
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        false
    }
}

/// Check if OpenCL is available (universal fallback)
pub fn is_opencl_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        // Check for clinfo or OpenCL ICD
        std::process::Command::new("clinfo")
            .output()
            .map(|o| o.status.success())
            .unwrap_or_else(|_| {
                // Fallback: check for OpenCL ICD directory
                std::path::Path::new("/etc/OpenCL/vendors").exists()
            })
    }
    #[cfg(target_os = "windows")]
    {
        // Check for OpenCL.dll
        std::path::Path::new("C:\\Windows\\System32\\OpenCL.dll").exists()
    }
    #[cfg(target_os = "macos")]
    {
        // OpenCL is deprecated on macOS but may still be available
        // Metal is strongly preferred
        std::path::Path::new("/System/Library/Frameworks/OpenCL.framework").exists()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        false
    }
}

/// Get list of all available GPU backends, sorted by preference (native first)
pub fn get_available_backends() -> Vec<GpuBackend> {
    let mut backends = Vec::new();
    
    // Native backends first (highest priority)
    if is_metal_available() {
        backends.push(GpuBackend::Metal);
    }
    if is_cuda_available() {
        backends.push(GpuBackend::Cuda);
    }
    if is_vulkan_available() {
        backends.push(GpuBackend::Vulkan);
    }
    
    // OpenCL as fallback (lower priority)
    if is_opencl_available() {
        backends.push(GpuBackend::OpenCL);
    }
    
    if backends.is_empty() {
        backends.push(GpuBackend::None);
    }
    
    // Sort by priority (native first, OpenCL last)
    backends.sort();
    backends
}

/// Get the best available GPU backend for the current system
/// Priority: Native APIs first, OpenCL as fallback
pub fn get_best_backend() -> GpuBackend {
    // Priority order (native first, OpenCL as fallback):
    // 1. Metal (macOS native - best on Apple Silicon)
    // 2. CUDA (NVIDIA native - best on NVIDIA GPUs)
    // 3. Vulkan (cross-platform native via wgpu)
    // 4. OpenCL (universal fallback)
    // 5. None (CPU only)
    
    if is_metal_available() {
        return GpuBackend::Metal;
    }
    if is_cuda_available() {
        return GpuBackend::Cuda;
    }
    if is_vulkan_available() {
        return GpuBackend::Vulkan;
    }
    if is_opencl_available() {
        return GpuBackend::OpenCL;
    }
    GpuBackend::None
}

/// Get the best backend for a specific GPU vendor
pub fn get_best_backend_for_vendor(vendor: &str) -> GpuBackend {
    let vendor_lower = vendor.to_lowercase();
    
    if vendor_lower.contains("apple") {
        if is_metal_available() {
            return GpuBackend::Metal;
        }
    }
    
    if vendor_lower.contains("nvidia") {
        // Prefer CUDA for NVIDIA, fall back to OpenCL
        if is_cuda_available() {
            return GpuBackend::Cuda;
        }
        if is_opencl_available() {
            return GpuBackend::OpenCL;
        }
    }
    
    if vendor_lower.contains("amd") || vendor_lower.contains("ati") {
        // AMD: prefer Vulkan (via wgpu), fall back to OpenCL
        if is_vulkan_available() {
            return GpuBackend::Vulkan;
        }
        if is_opencl_available() {
            return GpuBackend::OpenCL;
        }
    }
    
    if vendor_lower.contains("intel") {
        // Intel: prefer Vulkan (via wgpu), fall back to OpenCL
        if is_vulkan_available() {
            return GpuBackend::Vulkan;
        }
        if is_opencl_available() {
            return GpuBackend::OpenCL;
        }
    }
    
    // Unknown vendor: use best available
    get_best_backend()
}

/// Check if any GPU acceleration is available
pub fn is_gpu_available() -> bool {
    is_metal_available() || is_cuda_available() || is_vulkan_available() || is_opencl_available()
}

/// Print GPU detection summary
pub fn print_gpu_summary() {
    println!("GPU Detection Summary (native first, OpenCL fallback):");
    println!("  Metal:   {} {}", 
             if is_metal_available() { "✓ Available" } else { "✗ Not available" },
             if is_metal_available() { "(native)" } else { "" });
    println!("  CUDA:    {} {}", 
             if is_cuda_available() { "✓ Available" } else { "✗ Not available" },
             if is_cuda_available() { "(native)" } else { "" });
    println!("  Vulkan:  {} {}", 
             if is_vulkan_available() { "✓ Available" } else { "✗ Not available" },
             if is_vulkan_available() { "(native)" } else { "" });
    println!("  OpenCL:  {} {}", 
             if is_opencl_available() { "✓ Available" } else { "✗ Not available" },
             if is_opencl_available() { "(fallback)" } else { "" });
    println!("  AMD GPU: {}", if is_amd_available() { "✓ Detected" } else { "✗ Not detected" });
    println!("  Intel:   {}", if is_intel_gpu_available() { "✓ Detected" } else { "✗ Not detected" });
    println!("  Best:    {}", get_best_backend());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_backend_display() {
        assert_eq!(format!("{}", GpuBackend::Metal), "Metal");
        assert_eq!(format!("{}", GpuBackend::Cuda), "CUDA");
        assert_eq!(format!("{}", GpuBackend::Vulkan), "Vulkan");
        assert_eq!(format!("{}", GpuBackend::OpenCL), "OpenCL");
        assert_eq!(format!("{}", GpuBackend::None), "None");
    }

    #[test]
    fn test_gpu_backend_ordering() {
        // Verify native backends have higher priority (lower ord value) than OpenCL
        assert!(GpuBackend::Metal < GpuBackend::OpenCL, "Metal should have higher priority than OpenCL");
        assert!(GpuBackend::Cuda < GpuBackend::OpenCL, "CUDA should have higher priority than OpenCL");
        assert!(GpuBackend::Vulkan < GpuBackend::OpenCL, "Vulkan should have higher priority than OpenCL");
        assert!(GpuBackend::OpenCL < GpuBackend::None, "OpenCL should have higher priority than None");
    }

    #[test]
    fn test_get_available_backends() {
        let backends = get_available_backends();
        assert!(!backends.is_empty(), "Should have at least one backend (even if None)");
        
        // Verify backends are sorted by priority (native first)
        for i in 1..backends.len() {
            assert!(backends[i-1] <= backends[i], "Backends should be sorted by priority");
        }
    }

    #[test]
    fn test_get_best_backend() {
        let best = get_best_backend();
        // Just verify it returns something valid
        match best {
            GpuBackend::Metal | GpuBackend::Cuda | GpuBackend::Vulkan | 
            GpuBackend::OpenCL | GpuBackend::None => {}
        }
        
        // If any GPU is available, best should not be None
        if is_gpu_available() {
            assert_ne!(best, GpuBackend::None, "Should return a GPU backend when available");
        }
    }

    #[test]
    fn test_is_gpu_available() {
        let available = is_gpu_available();
        let best = get_best_backend();
        
        // Consistency check
        if available {
            assert_ne!(best, GpuBackend::None);
        } else {
            assert_eq!(best, GpuBackend::None);
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
            
            // On macOS with Metal, it should be the best backend
            assert_eq!(get_best_backend(), GpuBackend::Metal);
        }
    }

    #[test]
    fn test_cuda_detection() {
        // Just verify it doesn't panic
        let _available = is_cuda_available();
    }

    #[test]
    fn test_vulkan_detection() {
        // Just verify it doesn't panic
        let _available = is_vulkan_available();
    }

    #[test]
    fn test_opencl_detection() {
        // Just verify it doesn't panic
        let _available = is_opencl_available();
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

    #[test]
    fn test_vendor_backend_selection() {
        // Test that vendor-specific backend selection prefers native APIs
        let nvidia_backend = get_best_backend_for_vendor("NVIDIA");
        if is_cuda_available() {
            assert_eq!(nvidia_backend, GpuBackend::Cuda, "NVIDIA should prefer CUDA");
        }
        
        // Apple should prefer Metal
        let apple_backend = get_best_backend_for_vendor("Apple");
        if is_metal_available() {
            assert_eq!(apple_backend, GpuBackend::Metal, "Apple should prefer Metal");
        }
    }
}
