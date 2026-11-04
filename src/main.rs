mod config;
mod error;
mod service;
mod package;
mod installer;
mod cache;

use config::Config;
use service::Service;
use package::{PackageRepository, Package, Version};
use installer::PackageInstaller;
use cache::PackageCache;
use anyhow::Result;

fn main() -> Result<()> {
    env_logger::init();
    
    let config = Config::default();
    let mut service = Service::new(config);
    
    service.start()?;
    println!("Service started successfully");
    
    // Demonstrate package management
    let mut repo = PackageRepository::new();
    let pkg = Package::new("test-pkg".to_string(), Version::new(1, 0, 0), "Test package".to_string());
    repo.add_package(pkg);
    println!("Package repository: {} packages", repo.list_packages().len());
    
    // Demonstrate installation
    let installer = PackageInstaller::default();
    let mut transaction = installer.create_transaction("test-pkg".to_string());
    installer.install(&mut transaction)?;
    println!("Installation transaction completed");
    
    // Demonstrate cache
    let mut cache = PackageCache::default();
    println!("Package cache initialized");
    
    service.stop()?;
    println!("Service stopped successfully");
    
    Ok(())
}
