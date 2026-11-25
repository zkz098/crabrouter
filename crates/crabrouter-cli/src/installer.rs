use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use serde::Serialize;

const ESP_PARTTYPE_GUID: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";
const LIVE_LABEL_DEV: &str = "/dev/disk/by-label/CRABROUTER_LIVE";
const DEFAULT_MEDIA_DIRS: &[&str] = &[
    "/run/crabrouter-live",
    "/mnt/crabrouter-live",
    "/media/crabrouter",
];

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
pub struct InstallOptions {
    pub target: PathBuf,
    pub repart_dir: PathBuf,
    pub root_image: Option<PathBuf>,
    pub uki_image: Option<PathBuf>,
    pub media_dir: Option<PathBuf>,
    pub version: Option<String>,
    pub seed_config: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub target: PathBuf,
    pub repart_dir: PathBuf,
    pub root_image: Option<PathBuf>,
    pub uki_image: Option<PathBuf>,
    pub media_dir: Option<PathBuf>,
    pub explicit_version: Option<String>,
    pub version: String,
    pub seed_config: Option<PathBuf>,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TargetPartitions {
    esp: PathBuf,
    root_a: PathBuf,
    state: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveredMedia {
    root_image: Option<PathBuf>,
    uki_image: Option<PathBuf>,
    version: Option<String>,
}

struct TempMountGuard {
    mount_point: PathBuf,
    mounted: bool,
}

impl TempMountGuard {
    fn mount_device(device: &Path, tag: &str, read_only: bool) -> anyhow::Result<Self> {
        let mount_point =
            std::env::temp_dir().join(format!("crabrouter-{tag}-{}", std::process::id()));
        fs::create_dir_all(&mount_point)?;
        let mut cmd = Command::new("mount");
        if read_only {
            cmd.args(["-o", "ro"]);
        }
        let status = cmd
            .arg(device)
            .arg(&mount_point)
            .status()
            .with_context(|| format!("failed to mount {}", device.display()))?;
        if !status.success() {
            let _ = fs::remove_dir_all(&mount_point);
            bail!("mount {} failed with {status}", device.display());
        }
        Ok(Self {
            mount_point,
            mounted: true,
        })
    }

    fn path(&self) -> &Path {
        &self.mount_point
    }
}

impl Drop for TempMountGuard {
    fn drop(&mut self) {
        if self.mounted {
            let _ = Command::new("umount").arg(&self.mount_point).status();
            self.mounted = false;
        }
        let _ = fs::remove_dir_all(&self.mount_point);
    }
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

pub fn build_install_plan(opts: &InstallOptions) -> anyhow::Result<InstallPlan> {
    let target_str = opts.target.to_string_lossy();
    if !target_str.starts_with("/dev/") {
        bail!("target disk must be a /dev/* block device (got {target_str})");
    }
    let version = resolve_version_label(opts.version.as_deref(), None, opts.uki_image.as_deref());
    let uki_entry_name = format_boot_assessment_uki_name(&version);
    let steps = vec![
        format!(
            "Verify target {} has no active mounts and run systemd-repart --definitions={} --empty=force --dry-run=no {}",
            opts.target.display(),
            opts.repart_dir.display(),
            opts.target.display()
        ),
        "Run udevadm settle and resolve ESP, crabrouter_root_a, and crabrouter_state partitions on target disk"
            .to_string(),
        format!(
            "Write immutable squashfs rootfs to target crabrouter_root_a{}",
            opts.root_image.as_deref().map_or_else(
                || " (auto-discovered from Live ISO CRABROUTER_LIVE)".to_string(),
                |p| format!(" (from {})", p.display())
            )
        ),
        format!(
            "Mount target ESP, run bootctl install, write loader.conf, and stage UKI as /EFI/Linux/{uki_entry_name}{}",
            opts.uki_image.as_deref().map_or_else(
                || " (auto-discovered from Live ISO CRABROUTER_LIVE)".to_string(),
                |p| format!(" (from {})", p.display())
            )
        ),
        format!(
            "Mount target crabrouter_state and initialize /var/lib/crabrouter{}",
            opts.seed_config.as_deref().map_or_else(
                String::new,
                |p| format!(" (seeding router.toml from {})", p.display())
            )
        ),
    ];
    Ok(InstallPlan {
        target: opts.target.clone(),
        repart_dir: opts.repart_dir.clone(),
        root_image: opts.root_image.clone(),
        uki_image: opts.uki_image.clone(),
        media_dir: opts.media_dir.clone(),
        explicit_version: opts.version.clone(),
        version,
        seed_config: opts.seed_config.clone(),
        steps,
    })
}

pub fn execute_install(plan: &InstallPlan, dry_run: bool, assume_yes: bool) -> anyhow::Result<()> {
    if dry_run {
        println!(
            "Dry-run install plan for {} (version {}):",
            plan.target.display(),
            plan.version
        );
        for (i, step) in plan.steps.iter().enumerate() {
            println!("  {}. {step}", i + 1);
        }
        return Ok(());
    }

    validate_install_prerequisites(plan, assume_yes)?;
    let (root_image, uki_image, version, _live_guard) = resolve_install_artifacts(plan)?;

    println!(
        "==> 1/5 Partitioning {} with systemd-repart...",
        plan.target.display()
    );
    run_systemd_repart(&plan.repart_dir, &plan.target)?;
    settle_block_devices(&plan.target);

    println!("==> 2/5 Resolving target disk partitions...");
    let parts = discover_target_partitions(&plan.target)?;

    println!(
        "==> 3/5 Writing rootfs {} -> {}...",
        root_image.display(),
        parts.root_a.display()
    );
    write_rootfs_image(&root_image, &parts.root_a)?;

    println!(
        "==> 4/5 Installing systemd-boot and UKI ({}) onto {}...",
        format_boot_assessment_uki_name(&version),
        parts.esp.display()
    );
    install_esp_and_uki(&parts.esp, &uki_image, &version)?;

    println!(
        "==> 5/5 Initializing persistent state partition {}...",
        parts.state.display()
    );
    initialize_state_partition(&parts.state, plan.seed_config.as_deref())?;

    let _ = Command::new("sync").status();
    println!(
        "Installation complete on {} (version {version}). You may now remove the Live media and reboot.",
        plan.target.display()
    );
    Ok(())
}

fn validate_install_prerequisites(plan: &InstallPlan, assume_yes: bool) -> anyhow::Result<()> {
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
    if let Some(seed) = plan.seed_config.as_deref()
        && !seed.exists()
    {
        bail!("seed config {} does not exist", seed.display());
    }
    ensure_target_not_mounted(&plan.target)
}

fn ensure_target_not_mounted(target: &Path) -> anyhow::Result<()> {
    let output = Command::new("lsblk")
        .args(["-nrpo", "MOUNTPOINT"])
        .arg(target)
        .output();
    let Ok(out) = output else {
        return Ok(());
    };
    if !out.status.success() {
        return Ok(());
    }
    let mounts: Vec<&str> = std::str::from_utf8(&out.stdout)
        .unwrap_or("")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if !mounts.is_empty() {
        bail!(
            "refusing to wipe {}: active mount points detected ({})",
            target.display(),
            mounts.join(", ")
        );
    }
    Ok(())
}

fn resolve_install_artifacts(
    plan: &InstallPlan,
) -> anyhow::Result<(PathBuf, PathBuf, String, Option<TempMountGuard>)> {
    let mut root_img = plan.root_image.clone();
    let mut uki_img = plan.uki_image.clone();
    let mut discovered_version = None;
    let mut live_guard = None;

    if root_img.is_none() || uki_img.is_none() {
        let mut search_dirs = Vec::new();
        if let Some(dir) = plan.media_dir.as_ref() {
            search_dirs.push(dir.clone());
        }
        for dir in DEFAULT_MEDIA_DIRS {
            search_dirs.push(PathBuf::from(dir));
        }

        for dir in &search_dirs {
            let found = scan_media_dir(dir);
            merge_discovered(&mut root_img, &mut uki_img, &mut discovered_version, found);
            if root_img.is_some() && uki_img.is_some() {
                break;
            }
        }
    }

    if (root_img.is_none() || uki_img.is_none()) && Path::new(LIVE_LABEL_DEV).exists() {
        let guard = TempMountGuard::mount_device(Path::new(LIVE_LABEL_DEV), "live-media", true)?;
        let found = scan_media_dir(guard.path());
        merge_discovered(&mut root_img, &mut uki_img, &mut discovered_version, found);
        live_guard = Some(guard);
    }

    let root = root_img.ok_or_else(|| {
        anyhow::anyhow!(
            "could not locate rootfs image (crabrouter*.root-x86-64.raw); pass --root-image or boot from CRABROUTER_LIVE media"
        )
    })?;
    let uki = uki_img.ok_or_else(|| {
        anyhow::anyhow!(
            "could not locate disk UKI image (crabrouter*.efi); pass --uki-image or boot from CRABROUTER_LIVE media"
        )
    })?;
    if !root.exists() {
        bail!("rootfs image {} does not exist", root.display());
    }
    if !uki.exists() {
        bail!("UKI image {} does not exist", uki.display());
    }

    let version = resolve_version_label(
        plan.explicit_version.as_deref(),
        discovered_version.as_deref(),
        Some(uki.as_path()),
    );
    Ok((root, uki, version, live_guard))
}

fn merge_discovered(
    root_img: &mut Option<PathBuf>,
    uki_img: &mut Option<PathBuf>,
    version: &mut Option<String>,
    found: DiscoveredMedia,
) {
    if root_img.is_none() {
        *root_img = found.root_image;
    }
    if uki_img.is_none() {
        *uki_img = found.uki_image;
    }
    if version.is_none() {
        *version = found.version;
    }
}

fn scan_media_dir(base: &Path) -> DiscoveredMedia {
    let mut discovered = DiscoveredMedia {
        root_image: None,
        uki_image: None,
        version: None,
    };
    for candidate_dir in [base.join("crabrouter"), base.to_path_buf()] {
        if !candidate_dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&candidate_dir) else {
            continue;
        };
        let mut names: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        names.sort();
        for path in names {
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if discovered.version.is_none() && name == "VERSION" {
                discovered.version = fs::read_to_string(&path)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
            } else if discovered.root_image.is_none()
                && name.starts_with("crabrouter")
                && name.ends_with(".root-x86-64.raw")
            {
                discovered.root_image = Some(path);
            } else if discovered.uki_image.is_none()
                && name.starts_with("crabrouter")
                && !name.starts_with("crabrouter-live")
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("efi"))
            {
                discovered.uki_image = Some(path);
            }
        }
    }
    discovered
}

fn run_systemd_repart(repart_dir: &Path, target: &Path) -> anyhow::Result<()> {
    let status = Command::new("systemd-repart")
        .arg(format!("--definitions={}", repart_dir.display()))
        .arg("--empty=force")
        .arg("--dry-run=no")
        .arg(target)
        .status()
        .context("failed to execute systemd-repart")?;
    if !status.success() {
        bail!("systemd-repart failed with exit status {status}");
    }
    Ok(())
}

fn settle_block_devices(target: &Path) {
    let _ = Command::new("partprobe").arg(target).status();
    let _ = Command::new("udevadm")
        .args(["settle", "--timeout=15"])
        .status();
}

fn discover_target_partitions(target: &Path) -> anyhow::Result<TargetPartitions> {
    let output = Command::new("lsblk")
        .args(["-P", "-o", "PATH,PARTTYPE,PARTLABEL"])
        .arg(target)
        .output()
        .context("failed to execute lsblk to inspect target partitions")?;
    if !output.status.success() {
        bail!(
            "lsblk failed on {}: {}",
            target.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_target_partitions(&text, target)
}

fn parse_target_partitions(lsblk_output: &str, target: &Path) -> anyhow::Result<TargetPartitions> {
    let mut esp = None;
    let mut root_a = None;
    let mut state = None;

    for line in lsblk_output.lines() {
        let pairs = parse_lsblk_key_values(line);
        let Some(path) = pairs.get("PATH").filter(|p| !p.is_empty()) else {
            continue;
        };
        let parttype = pairs.get("PARTTYPE").map_or("", String::as_str);
        let partlabel = pairs.get("PARTLABEL").map_or("", String::as_str);

        if esp.is_none()
            && (parttype.eq_ignore_ascii_case(ESP_PARTTYPE_GUID)
                || partlabel.eq_ignore_ascii_case("esp")
                || partlabel.eq_ignore_ascii_case("EFI System Partition"))
        {
            esp = Some(PathBuf::from(path));
        } else if root_a.is_none() && partlabel == "crabrouter_root_a" {
            root_a = Some(PathBuf::from(path));
        } else if state.is_none() && partlabel == "crabrouter_state" {
            state = Some(PathBuf::from(path));
        }
    }

    let esp = esp
        .ok_or_else(|| anyhow::anyhow!("ESP partition not found on target {}", target.display()))?;
    let root_a = root_a.ok_or_else(|| {
        anyhow::anyhow!(
            "PARTLABEL=crabrouter_root_a not found on target {}",
            target.display()
        )
    })?;
    let state = state.ok_or_else(|| {
        anyhow::anyhow!(
            "PARTLABEL=crabrouter_state not found on target {}",
            target.display()
        )
    })?;

    Ok(TargetPartitions { esp, root_a, state })
}

fn parse_lsblk_key_values(line: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut rest = line.trim();
    while !rest.is_empty() {
        let Some(eq_pos) = rest.find('=') else {
            break;
        };
        let key = rest[..eq_pos].trim().to_string();
        rest = &rest[eq_pos + 1..];
        let Some(after_open) = rest.strip_prefix('"') else {
            break;
        };
        let mut val = String::new();
        let mut chars = after_open.char_indices();
        let mut end_idx = after_open.len();
        while let Some((idx, ch)) = chars.next() {
            if ch == '\\' {
                if let Some((_, escaped)) = chars.next() {
                    val.push(escaped);
                }
            } else if ch == '"' {
                end_idx = idx + 1;
                break;
            } else {
                val.push(ch);
            }
        }
        map.insert(key, val);
        rest = after_open[end_idx..].trim_start();
    }
    map
}

fn write_rootfs_image(root_img: &Path, slot_a: &Path) -> anyhow::Result<()> {
    let status = Command::new("dd")
        .arg(format!("if={}", root_img.display()))
        .arg(format!("of={}", slot_a.display()))
        .arg("bs=4M")
        .arg("conv=fsync")
        .status()
        .context("failed to write rootfs image to crabrouter_root_a")?;
    if !status.success() {
        bail!("dd to {} failed with {status}", slot_a.display());
    }
    Ok(())
}

fn install_esp_and_uki(esp_dev: &Path, uki_src: &Path, version: &str) -> anyhow::Result<()> {
    let esp = TempMountGuard::mount_device(esp_dev, "esp", false)?;
    run_bootctl_install(esp.path())?;

    let loader_dir = esp.path().join("loader");
    fs::create_dir_all(&loader_dir)?;
    fs::write(
        loader_dir.join("loader.conf"),
        "default crabrouter*.efi\ntimeout 3\nconsole-mode max\neditor no\n",
    )?;

    let linux_dir = esp.path().join("EFI").join("Linux");
    fs::create_dir_all(&linux_dir)?;
    let uki_dst = linux_dir.join(format_boot_assessment_uki_name(version));
    let copied = fs::copy(uki_src, &uki_dst).with_context(|| {
        format!(
            "failed to copy UKI {} to {}",
            uki_src.display(),
            uki_dst.display()
        )
    })?;
    if copied == 0 {
        bail!("copied UKI {} is empty", uki_dst.display());
    }
    Ok(())
}

fn run_bootctl_install(esp_mount: &Path) -> anyhow::Result<()> {
    let esp_arg = format!("--esp-path={}", esp_mount.display());
    let status = Command::new("bootctl")
        .arg(&esp_arg)
        .arg("install")
        .status()
        .context("failed to run bootctl install")?;
    if status.success() {
        return Ok(());
    }
    eprintln!(
        "warn: bootctl install returned {status}; retrying with --no-variables (UEFI fallback path EFI/BOOT/BOOTX64.EFI remains active)"
    );
    let fallback = Command::new("bootctl")
        .arg(&esp_arg)
        .arg("--no-variables")
        .arg("install")
        .status()
        .context("failed to run bootctl --no-variables install")?;
    if !fallback.success() {
        bail!("bootctl install failed with {fallback}");
    }
    Ok(())
}

fn initialize_state_partition(state_dev: &Path, seed_config: Option<&Path>) -> anyhow::Result<()> {
    let state = TempMountGuard::mount_device(state_dev, "state", false)?;
    let crab_dir = state.path().join("lib").join("crabrouter");
    fs::create_dir_all(crab_dir.join("etc-overlay").join("upper"))?;
    fs::create_dir_all(crab_dir.join("etc-overlay").join("work"))?;
    if let Some(cfg_src) = seed_config {
        fs::copy(cfg_src, crab_dir.join("router.toml"))
            .with_context(|| format!("failed to seed router.toml from {}", cfg_src.display()))?;
    }
    Ok(())
}

pub fn format_boot_assessment_uki_name(version: &str) -> String {
    let clean = version.trim().trim_start_matches('v');
    let ver = if clean.is_empty() {
        env!("CARGO_PKG_VERSION")
    } else {
        clean
    };
    format!("crabrouter_{ver}+3-0.efi")
}

fn resolve_version_label(
    explicit: Option<&str>,
    media_version: Option<&str>,
    uki_path: Option<&Path>,
) -> String {
    if let Some(v) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        return v.to_string();
    }
    if let Some(v) = media_version.map(str::trim).filter(|s| !s.is_empty()) {
        return v.to_string();
    }
    if let Some(stem) = uki_path
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        && let Some(rest) = stem.strip_prefix("crabrouter_")
    {
        let ver = rest.split('+').next().unwrap_or("").trim();
        if !ver.is_empty() {
            return ver.to_string();
        }
    }
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_repart_install_plan_and_rejects_non_dev() {
        let bad = InstallOptions {
            target: PathBuf::from("/tmp/disk.img"),
            repart_dir: PathBuf::from("/usr/share/crabrouter/repart.d"),
            root_image: None,
            uki_image: None,
            media_dir: None,
            version: None,
            seed_config: None,
        };
        assert!(build_install_plan(&bad).is_err());

        let good = InstallOptions {
            target: PathBuf::from("/dev/nvme0n1"),
            repart_dir: PathBuf::from("/usr/share/crabrouter/repart.d"),
            root_image: None,
            uki_image: Some(PathBuf::from("/mnt/crabrouter/crabrouter_0.2.0.efi")),
            media_dir: None,
            version: None,
            seed_config: None,
        };
        let plan = build_install_plan(&good).unwrap();
        assert_eq!(plan.steps.len(), 5);
        assert_eq!(plan.version, "0.2.0");
        assert!(plan.steps[0].contains("systemd-repart"));
        assert!(plan.steps[3].contains("crabrouter_0.2.0+3-0.efi"));
    }

    #[test]
    fn parses_target_partitions_from_lsblk_pairs() {
        let sample = r#"
PATH="/dev/nvme0n1" PARTTYPE="" PARTLABEL=""
PATH="/dev/nvme0n1p1" PARTTYPE="c12a7328-f81f-11d2-ba4b-00a0c93ec93b" PARTLABEL="EFI System Partition"
PATH="/dev/nvme0n1p2" PARTTYPE="4f68bce3-e8cd-4db1-96e7-fbcaf984b709" PARTLABEL="crabrouter_root_a"
PATH="/dev/nvme0n1p3" PARTTYPE="4f68bce3-e8cd-4db1-96e7-fbcaf984b709" PARTLABEL="crabrouter_root_b"
PATH="/dev/nvme0n1p4" PARTTYPE="4d21b016-b534-45c2-a9fb-5c16e091fd2d" PARTLABEL="crabrouter_state"
"#;
        let parts = parse_target_partitions(sample, Path::new("/dev/nvme0n1")).unwrap();
        assert_eq!(parts.esp, PathBuf::from("/dev/nvme0n1p1"));
        assert_eq!(parts.root_a, PathBuf::from("/dev/nvme0n1p2"));
        assert_eq!(parts.state, PathBuf::from("/dev/nvme0n1p4"));
    }

    #[test]
    fn scans_live_media_directory_and_skips_live_uki() {
        let tmp = std::env::temp_dir().join(format!(
            "crabrouter-installer-media-test-{}",
            std::process::id()
        ));
        let sub = tmp.join("crabrouter");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("VERSION"), "0.3.1\n").unwrap();
        fs::write(sub.join("crabrouter-live_0.3.1.efi"), b"live").unwrap();
        fs::write(sub.join("crabrouter_0.3.1.efi"), b"disk-uki").unwrap();
        fs::write(sub.join("crabrouter_0.3.1.root-x86-64.raw"), b"rootfs").unwrap();

        let found = scan_media_dir(&tmp);
        assert_eq!(found.version.as_deref(), Some("0.3.1"));
        assert_eq!(found.uki_image, Some(sub.join("crabrouter_0.3.1.efi")));
        assert_eq!(
            found.root_image,
            Some(sub.join("crabrouter_0.3.1.root-x86-64.raw"))
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn formats_boot_assessment_uki_filename() {
        assert_eq!(
            format_boot_assessment_uki_name("0.1.0"),
            "crabrouter_0.1.0+3-0.efi"
        );
        assert_eq!(
            format_boot_assessment_uki_name("v1.2.3"),
            "crabrouter_1.2.3+3-0.efi"
        );
    }
}
