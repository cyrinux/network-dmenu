use std::process::Command;
use std::path::PathBuf;
use std::fs;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct Options {
    #[command(subcommand)]
    command: Option<XTaskCommand>,
}

#[derive(Debug, Parser)]
pub enum XTaskCommand {
    BuildEbpf(BuildEbpfOptions),
    BuildAll(BuildAllOptions),
}

#[derive(Debug, Parser)]
pub struct BuildEbpfOptions {
    /// Set the endianness of the BPF target
    #[clap(default_value = "bpfel-unknown-none", long)]
    pub target: String,
    /// Build the release target
    #[clap(long)]
    pub release: bool,
}

#[derive(Debug, Parser)]
pub struct BuildAllOptions {
    /// Build the release target
    #[clap(long)]
    pub release: bool,
}

pub fn build_ebpf(opts: BuildEbpfOptions) -> Result<(), anyhow::Error> {
    let dir = std::env::current_dir()?;
    let target = format!("--target={}", opts.target);
    
    println!("Building eBPF program...");
    
    let mut args = vec![
        "build",
        target.as_str(),
        "-Z",
        "build-std=core",
    ];
    
    if opts.release {
        args.push("--release");
    }
    
    let status = Command::new("cargo")
        .current_dir(dir.join("network-monitor-ebpf"))
        .args(&args)
        .status()
        .expect("failed to build eBPF program");
    
    if !status.success() {
        anyhow::bail!("Failed to build eBPF program");
    }
    
    // Copy the built eBPF program to the target directory where the userspace program expects it
    let profile = if opts.release { "release" } else { "debug" };
    let ebpf_binary_path = dir.join(format!(
        "target/{}/{}/network-monitor", 
        opts.target, 
        profile
    ));
    
    let target_dir = dir.join("target/bpf");
    fs::create_dir_all(&target_dir)?;
    
    let target_path = target_dir.join("network-monitor");
    
    if ebpf_binary_path.exists() {
        fs::copy(&ebpf_binary_path, &target_path)?;
        println!("✅ eBPF program built and copied to {}", target_path.display());
    } else {
        println!("⚠️  eBPF binary not found at {}, check build", ebpf_binary_path.display());
    }
    
    Ok(())
}

pub fn build_all(opts: BuildAllOptions) -> Result<(), anyhow::Error> {
    // First build the eBPF program
    println!("🔧 Building eBPF program first...");
    build_ebpf(BuildEbpfOptions {
        target: "bpfel-unknown-none".to_owned(),
        release: opts.release,
    })?;
    
    // Then build the userspace program with BPF features
    println!("🔧 Building userspace program with BPF features...");
    let mut args = vec!["build", "--features", "bpf"];
    if opts.release {
        args.push("--release");
    }
    
    let status = Command::new("cargo")
        .args(&args)
        .status()
        .expect("failed to build userspace program");
    
    if !status.success() {
        anyhow::bail!("Failed to build userspace program");
    }
    
    println!("✅ Successfully built both eBPF and userspace programs");
    Ok(())
}

fn main() {
    let opts = Options::parse();

    use XTaskCommand::*;
    let ret = match opts.command {
        Some(BuildEbpf(opts)) => build_ebpf(opts),
        Some(BuildAll(opts)) => build_all(opts),
        None => build_ebpf(BuildEbpfOptions {
            target: "bpfel-unknown-none".to_owned(),
            release: false,
        }),
    };

    if let Err(e) = ret {
        eprintln!("❌ Error: {e:#}");
        std::process::exit(1);
    }
}