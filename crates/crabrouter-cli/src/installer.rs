use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BlockDiskInfo {
    pub name: String,
    pub device_path: String,
    pub size_bytes: u64,
    pub size_gb: f64,
    pub model: String,
    pub removable: bool,
}

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub target: PathBuf,
    pub repart_dir: PathBuf,
    pub root_image: Option<PathBuf>,
    pub steps: Vec<String>,
}

/// Discovers physical block disks from `/sys/block` (excluding `loop`, `ram`, `zram`, `dm-`).
#[allow(clippy::cast_precision_loss)]
pub fn list_block_disks(sys_block_dir: &Path) -> Vec<BlockDiskInfo> {
    let Ok(entries) = fs::read_dir(sys_block_dir) else {
        return Vec::new();
    };
    let mut disks = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("loop")
            || name.starts_with("ram")
            || name.starts_with("zram")
            || name.starts_with("dm-")
        {
            continue;
        }
        let base = entry.path();
        let sectors = fs::read_to_string(base.join("size"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        if sectors == 0 {
            continue;
        }
        let size_bytes = sectors.saturating_mul(512);
        let model = fs::read_to_string(base.join("device/model"))
            .map_or_else(|_| "unknown".to_string(), |s| s.trim().to_string());
        let removable = fs::read_to_string(base.join("removable")).is_ok_and(|s| s.trim() == "1");

        disks.push(BlockDiskInfo {
            device_path: format!("/dev/{name}"),
            name,
            size_bytes,
            size_gb: size_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            model,
            removable,
        });
    }
    disks.sort_by(|a, b| a.name.cmp(&b.name));
    disks
}

pub fn build_install_plan(
    target: &Path,
    repart_dir: &Path,
    root_image: Option<&Path>,
) -> anyhow::Result<InstallPlan> {
    let target_str = target.to_string_lossy();
    if !target_str.starts_with("/dev/") {
        bail!("target disk must be a /dev/* block device (got {target_str})");
    }
    let steps = vec![
        format!(
            "systemd-repart --definitions={} --empty=force --dry-run=no {}",
            repart_dir.display(),
            target.display()
        ),
        format!(
            "Write immutable squashfs rootfs to PARTLABEL=crabrouter_root_a{}",
            root_image.map_or_else(
                || " (from Live USB root)".to_string(),
                |p| format!(" (from {})", p.display())
            )
        ),
        "Mount ESP (EFI System Partition) and run: bootctl --esp-path=<esp> install".to_string(),
        "Write <esp>/loader/loader.conf (timeout 3, default crabrouter_*) and UKI +3-0 Boot Assessment entry"
            .to_string(),
        "Format/verify PARTLABEL=crabrouter_state and seed /var/lib/crabrouter".to_string(),
    ];
    Ok(InstallPlan {
        target: target.to_path_buf(),
        repart_dir: repart_dir.to_path_buf(),
        root_image: root_image.map(Path::to_path_buf),
        steps,
    })
}

pub fn execute_install(plan: &InstallPlan, dry_run: bool, assume_yes: bool) -> anyhow::Result<()> {
    if dry_run {
        println!("Dry-run install plan for {}:", plan.target.display());
        for (i, step) in plan.steps.iter().enumerate() {
            println!("  {}. {step}", i + 1);
        }
        return Ok(());
    }

    if !assume_yes {
        bail!(
            "refusing destructive disk wipe on {} without --yes",
            plan.target.display()
        );
    }
    if !plan.target.exists() {
        bail!(
            "target block device {} does not exist",
            plan.target.display()
        );
    }
    if !plan.repart_dir.exists() {
        bail!(
            "systemd-repart definitions directory {} not found",
            plan.repart_dir.display()
        );
    }

    let status = Command::new("systemd-repart")
        .arg(format!("--definitions={}", plan.repart_dir.display()))
        .arg("--empty=force")
        .arg("--dry-run=no")
        .arg(&plan.target)
        .status()
        .context("failed to execute systemd-repart")?;
    if !status.success() {
        bail!("systemd-repart failed with exit status {status}");
    }

    if let Some(root_img) = plan.root_image.as_deref() {
        let slot_a = Path::new("/dev/disk/by-partlabel/crabrouter_root_a");
        let dd_status = Command::new("dd")
            .arg(format!("if={}", root_img.display()))
            .arg(format!("of={}", slot_a.display()))
            .arg("bs=4M")
            .arg("conv=fsync")
            .status()
            .context("failed to write rootfs image to crabrouter_root_a")?;
        if !dd_status.success() {
            bail!("dd to crabrouter_root_a failed with {dd_status}");
        }
    }

    let esp_mount = std::env::temp_dir().join(format!("crabrouter-esp-{}", std::process::id()));
    fs::create_dir_all(&esp_mount)?;
    let esp_dev = Path::new("/dev/disk/by-partlabel/esp");
    let mount_target = if esp_dev.exists() {
        esp_dev
    } else {
        Path::new("/dev/disk/by-partlabel/EFI\\x20System\\x20Partition")
    };
    let m_status = Command::new("mount")
        .arg(mount_target)
        .arg(&esp_mount)
        .status()
        .context("failed to mount ESP partition")?;
    if !m_status.success() {
        let _ = fs::remove_dir_all(&esp_mount);
        bail!("mount ESP failed with {m_status}");
    }

    let install_res = install_boot_entries(&esp_mount);
    let _ = Command::new("umount").arg(&esp_mount).status();
    let _ = fs::remove_dir_all(&esp_mount);
    install_res
}

fn install_boot_entries(esp_mount: &Path) -> anyhow::Result<()> {
    let bootctl_status = Command::new("bootctl")
        .arg(format!("--esp-path={}", esp_mount.display()))
        .arg("install")
        .status()
        .context("failed to run bootctl install")?;
    if !bootctl_status.success() {
        bail!("bootctl install failed with {bootctl_status}");
    }

    let loader_dir = esp_mount.join("loader");
    fs::create_dir_all(&loader_dir)?;
    fs::write(
        loader_dir.join("loader.conf"),
        "default crabrouter_*.efi\ntimeout 3\nconsole-mode max\neditor no\n",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_repart_install_plan_and_rejects_non_dev() {
        assert!(
            build_install_plan(
                Path::new("/tmp/disk.img"),
                Path::new("/usr/lib/repart.d"),
                None
            )
            .is_err()
        );
        let plan = build_install_plan(
            Path::new("/dev/nvme0n1"),
            Path::new("/usr/lib/repart.d"),
            None,
        )
        .unwrap();
        assert_eq!(plan.steps.len(), 5);
        assert!(plan.steps[0].contains("systemd-repart"));
        assert!(plan.steps[3].contains("Boot Assessment"));
    }
}
