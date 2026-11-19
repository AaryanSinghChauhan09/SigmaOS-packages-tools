// SPDX-License-Identifier: MIT OR Apache-2.0
//! SigmaOS-packages-tools — Package install/remove CLI (captain/gdebi-style).
//!
//! Provides inspection of local `.deb` packages, control stanza parsing,
//! install plan resolution, dependency checks, and CLI argument parsing.
//!
//! Inspired by Omarchy's lightweight CLI tools, Hyprland's packaging standards,
//! and Catppuccin Mocha's terminal color formatting.
//!
//! Principles: strict control field parsing, pure `std`, zero `unsafe`.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

// ── Error types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PkgToolsError {
    InvalidDebFile(String),
    MissingControlField(String),
    MalformedControlStanza(String),
    DependencyConflict(String),
    InvalidCliOption(String),
}

impl fmt::Display for PkgToolsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDebFile(p) => write!(f, "invalid Debian package file: '{p}' (must end in .deb)"),
            Self::MissingControlField(fld) => write!(f, "missing required control field: '{fld}'"),
            Self::MalformedControlStanza(e) => write!(f, "malformed control stanza: {e}"),
            Self::DependencyConflict(msg) => write!(f, "dependency conflict: {msg}"),
            Self::InvalidCliOption(opt) => write!(f, "invalid CLI option: '{opt}'"),
        }
    }
}

impl std::error::Error for PkgToolsError {}

// ── Deb Package Metadata ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebPackageInfo {
    pub file_path: PathBuf,
    pub package_name: String,
    pub version: String,
    pub architecture: String,
    pub dependencies: Vec<String>,
    pub description: String,
}

impl DebPackageInfo {
    pub fn parse_control(file_path: impl AsRef<Path>, control_content: &str) -> Result<Self, PkgToolsError> {
        let path = file_path.as_ref().to_path_buf();
        if path.extension().and_then(|e| e.to_str()) != Some("deb") {
            return Err(PkgToolsError::InvalidDebFile(path.display().to_string()));
        }

        let mut fields = HashMap::new();
        for line in control_content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once(':') {
                fields.insert(k.trim().to_lowercase(), v.trim().to_string());
            }
        }

        let package_name = fields.remove("package")
            .ok_or_else(|| PkgToolsError::MissingControlField("Package".into()))?;
        let version = fields.remove("version")
            .ok_or_else(|| PkgToolsError::MissingControlField("Version".into()))?;
        let architecture = fields.remove("architecture")
            .unwrap_or_else(|| "all".into());
        let description = fields.remove("description")
            .unwrap_or_default();

        let dependencies = if let Some(dep_str) = fields.remove("depends") {
            dep_str.split(',')
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        Ok(Self {
            file_path: path,
            package_name,
            version,
            architecture,
            dependencies,
            description,
        })
    }
}

// ── Install Plan ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanAction {
    InstallLocal,
    DownloadAndInstallDependency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub package: String,
    pub action: PlanAction,
}

#[derive(Debug, Default)]
pub struct InstallPlan {
    pub target_package: String,
    pub steps: Vec<PlanStep>,
}

impl InstallPlan {
    pub fn build(deb: &DebPackageInfo, installed_packages: &[String]) -> Self {
        let mut steps = Vec::new();
        for dep in &deb.dependencies {
            let clean_dep = dep.split_whitespace().next().unwrap_or(dep);
            if !installed_packages.iter().any(|pkg| pkg == clean_dep) {
                steps.push(PlanStep {
                    package: clean_dep.to_string(),
                    action: PlanAction::DownloadAndInstallDependency,
                });
            }
        }
        steps.push(PlanStep {
            package: deb.package_name.clone(),
            action: PlanAction::InstallLocal,
        });

        Self {
            target_package: deb.package_name.clone(),
            steps,
        }
    }

    pub fn step_count(&self) -> usize {
        self.steps.len()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_control_stanza() {
        // Arrange
        let control = "Package: sigma-term\nVersion: 1.2.0\nArchitecture: amd64\nDepends: libc6 (>= 2.34), kitty\nDescription: SigmaOS default terminal";
        // Act
        let deb = DebPackageInfo::parse_control("sigma-term_1.2.0_amd64.deb", control).unwrap();
        // Assert
        assert_eq!(deb.package_name, "sigma-term");
        assert_eq!(deb.version, "1.2.0");
        assert_eq!(deb.architecture, "amd64");
        assert_eq!(deb.dependencies.len(), 2);
        assert_eq!(deb.description, "SigmaOS default terminal");
    }

    #[test]
    fn parse_rejects_non_deb_extension() {
        // Arrange
        let control = "Package: test\nVersion: 1.0\n";
        // Act
        let err = DebPackageInfo::parse_control("test.tar.gz", control);
        // Assert
        assert!(matches!(err, Err(PkgToolsError::InvalidDebFile(_))));
    }

    #[test]
    fn parse_missing_required_package_field() {
        // Arrange
        let control = "Version: 1.0.0\nArchitecture: all\n";
        // Act
        let err = DebPackageInfo::parse_control("test.deb", control);
        // Assert
        assert_eq!(err, Err(PkgToolsError::MissingControlField("Package".into())));
    }

    #[test]
    fn install_plan_resolves_missing_dependencies() {
        // Arrange
        let control = "Package: editor\nVersion: 0.1\nDepends: libfontconfig1, libc6\n";
        let deb = DebPackageInfo::parse_control("editor.deb", control).unwrap();
        let already_installed = vec!["libc6".to_string()];

        // Act
        let plan = InstallPlan::build(&deb, &already_installed);

        // Assert: libc6 is already installed, so only libfontconfig1 and editor
        assert_eq!(plan.step_count(), 2);
        assert_eq!(plan.steps[0].package, "libfontconfig1");
        assert_eq!(plan.steps[0].action, PlanAction::DownloadAndInstallDependency);
        assert_eq!(plan.steps[1].package, "editor");
        assert_eq!(plan.steps[1].action, PlanAction::InstallLocal);
    }

    #[test]
    fn install_plan_all_dependencies_satisfied() {
        // Arrange
        let control = "Package: app\nVersion: 1.0\nDepends: libc6\n";
        let deb = DebPackageInfo::parse_control("app.deb", control).unwrap();
        let already_installed = vec!["libc6".to_string()];

        // Act
        let plan = InstallPlan::build(&deb, &already_installed);

        // Assert
        assert_eq!(plan.step_count(), 1);
        assert_eq!(plan.steps[0].package, "app");
    }

    #[test]
    fn error_display_formatting() {
        // Arrange
        let err = PkgToolsError::MissingControlField("Version".into());
        // Act & Assert
        assert_eq!(format!("{err}"), "missing required control field: 'Version'");
    }
}
