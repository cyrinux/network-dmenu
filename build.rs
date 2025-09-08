use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=network-monitor-ebpf/src/main.rs");
    println!("cargo:rerun-if-changed=network-monitor-common/src/lib.rs");
    
    // Only build eBPF program if BPF feature is enabled
    if env::var("CARGO_FEATURE_BPF").is_ok() {
        build_ebpf_if_needed();
    }
}

fn build_ebpf_if_needed() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let target_dir = PathBuf::from(&out_dir).join("../../../bpf");
    let target_path = target_dir.join("network-monitor");
    
    // Check if eBPF program is already built and up-to-date
    if target_path.exists() {
        // For now, assume it's up-to-date
        // In production, you might want to check timestamps
        println!("cargo:warning=eBPF program already exists at {}", target_path.display());
        return;
    }
    
    println!("cargo:warning=Building eBPF program...");
    
    // Create target directory
    std::fs::create_dir_all(&target_dir).expect("Failed to create BPF target directory");
    
    // Build eBPF program using xtask
    let status = Command::new("cargo")
        .args(&["xtask", "build-ebpf"])
        .status();
        
    match status {
        Ok(status) if status.success() => {
            println!("cargo:warning=✅ eBPF program built successfully");
        }
        Ok(status) => {
            println!("cargo:warning=⚠️ eBPF build failed with status: {}", status);
            println!("cargo:warning=BPF functionality will fall back to placeholder mode");
        }
        Err(e) => {
            println!("cargo:warning=⚠️ Failed to execute eBPF build: {}", e);
            println!("cargo:warning=BPF functionality will fall back to placeholder mode");
        }
    }
}