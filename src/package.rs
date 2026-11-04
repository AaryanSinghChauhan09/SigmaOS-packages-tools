//! Package management with dependency resolution
//! Follows: Dependency Inversion, Modular Design, Separation of Concerns

use std::collections::{HashMap, HashSet};

/// Package version
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Version { major, minor, patch }
    }

    pub fn parse(version_str: &str) -> Result<Self, &'static str> {
        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.len() != 3 {
            return Err("Invalid version format");
        }
        let major = parts[0].parse().map_err(|_| "Invalid major version")?;
        let minor = parts[1].parse().map_err(|_| "Invalid minor version")?;
        let patch = parts[2].parse().map_err(|_| "Invalid patch version")?;
        Ok(Version::new(major, minor, patch))
    }
}

/// Package metadata
#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub version: Version,
    pub description: String,
    pub dependencies: Vec<String>,
    pub installed: bool,
}

impl Package {
    pub fn new(name: String, version: Version, description: String) -> Self {
        Package {
            name,
            version,
            description,
            dependencies: Vec::new(),
            installed: false,
        }
    }

    pub fn add_dependency(&mut self, dep: String) {
        self.dependencies.push(dep);
    }
}

/// Repository of packages
pub struct PackageRepository {
    packages: HashMap<String, Package>,
}

impl PackageRepository {
    pub fn new() -> Self {
        PackageRepository {
            packages: HashMap::new(),
        }
    }

    pub fn add_package(&mut self, package: Package) {
        let name = package.name.clone();
        self.packages.insert(name, package);
    }

    pub fn get_package(&self, name: &str) -> Option<&Package> {
        self.packages.get(name)
    }

    pub fn list_packages(&self) -> Vec<&Package> {
        self.packages.values().collect()
    }

    pub fn search(&self, query: &str) -> Vec<&Package> {
        self.packages.values()
            .filter(|p| p.name.contains(query) || p.description.contains(query))
            .collect()
    }
}

impl Default for PackageRepository {
    fn default() -> Self {
        Self::new()
    }
}

/// Dependency resolver
pub struct DependencyResolver {
    repository: PackageRepository,
}

impl DependencyResolver {
    pub fn new(repository: PackageRepository) -> Self {
        DependencyResolver { repository }
    }

    pub fn resolve_dependencies(&self, package_name: &str) -> Result<Vec<String>, &'static str> {
        let mut resolved = Vec::new();
        let mut visited = HashSet::new();
        self.resolve_recursive(package_name, &mut resolved, &mut visited)?;
        Ok(resolved)
    }

    fn resolve_recursive(
        &self,
        package_name: &str,
        resolved: &mut Vec<String>,
        visited: &mut HashSet<String>,
    ) -> Result<(), &'static str> {
        if visited.contains(package_name) {
            return Ok(()); // Already visited, avoid cycles
        }
        visited.insert(package_name.to_string());

        let package = self.repository.get_package(package_name)
            .ok_or("Package not found")?;

        for dep in &package.dependencies {
            self.resolve_recursive(dep, resolved, visited)?;
            if !resolved.contains(dep) {
                resolved.push(dep.clone());
            }
        }

        if !resolved.contains(package_name) {
            resolved.push(package_name.to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        let version = Version::parse("1.2.3").unwrap();
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);
    }

    #[test]
    fn test_dependency_resolution() {
        let mut repo = PackageRepository::new();
        
        let mut pkg_a = Package::new("pkg-a".to_string(), Version::new(1, 0, 0), "Package A".to_string());
        pkg_a.add_dependency("pkg-b".to_string());
        
        let pkg_b = Package::new("pkg-b".to_string(), Version::new(1, 0, 0), "Package B".to_string());
        
        repo.add_package(pkg_a);
        repo.add_package(pkg_b);
        
        let resolver = DependencyResolver::new(repo);
        let deps = resolver.resolve_dependencies("pkg-a").unwrap();
        assert!(deps.contains(&"pkg-b".to_string()));
    }
}
