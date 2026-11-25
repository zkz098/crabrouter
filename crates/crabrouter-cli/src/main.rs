mod installer;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use crabrouter_core::ConfigStore;
use crabrouter_model::InterfacePin;
use crabrouter_net::{
    discover_physical_nic_pins, read_host_interfaces, render_nftables_ruleset,
    sync_interface_pin_files,
};
use crabrouter_services::{bless_current_boot, render_crabdns_toml, render_dnsmasq_conf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::installer::{InstallOptions, build_install_plan, execute_install, list_block_disks};

#[derive(Debug, Parser)]
#[command(name = "crabctl", about = "CrabRouter CLI management & installer tool")]
struct Cli {
    #[arg(short, long, default_value = "./data/router.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Validate router.toml configuration
    Validate,
    /// Print host network interfaces observed via sysfs/procfs
    Interfaces,
    /// Pin physical NIC names by MAC address to prevent ethX ordering drift across reboots
    PinNics {
        /// Optional explicit mappings in `name=aa:bb:cc:dd:ee:ff` form (repeatable)
        #[arg(long = "map")]
        mappings: Vec<String>,
        /// Write generated systemd.link and udev rules to disk
        #[arg(long)]
        apply: bool,
        #[arg(long, default_value = "/etc/systemd/network")]
        systemd_network_dir: PathBuf,
        #[arg(long, default_value = "/etc/udev/rules.d/70-crabrouter-net.rules")]
        udev_rules_path: PathBuf,
    },
    /// Preview generated nftables, dnsmasq, or crabdns configs
    Render {
        #[arg(value_enum)]
        target: RenderTarget,
    },
    /// List candidate NVMe/SATA block disks for appliance installation
    Disks,
    /// Install `CrabRouter` onto a target disk using systemd-repart A/B layout
    Install {
        #[arg(long)]
        target: PathBuf,
        #[arg(long, default_value = "/usr/share/crabrouter/repart.d")]
        repart_dir: PathBuf,
        #[arg(long)]
        root_image: Option<PathBuf>,
        #[arg(long)]
        uki_image: Option<PathBuf>,
        #[arg(long)]
        media_dir: Option<PathBuf>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        seed_config: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Manage A/B OTA updates and systemd-boot Boot Assessment
    Ota {
        #[command(subcommand)]
        action: OtaCommands,
    },
}

#[derive(Debug, Subcommand)]
enum OtaCommands {
    /// Verify local crabrouterd health and mark current boot slot as good
    Bless {
        #[arg(long, default_value = "127.0.0.1:8080")]
        health_addr: String,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum RenderTarget {
    Nftables,
    Dnsmasq,
    Crabdns,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Validate => {
            let store = ConfigStore::new(&cli.config);
            let cfg = store.load_or_init()?;
            println!(
                "OK: {} (hostname={}, wans={}, lans={})",
                cli.config.display(),
                cfg.system.hostname,
                cfg.wans.len(),
                cfg.lans.len()
            );
        }
        Commands::Interfaces => {
            let ifaces = read_host_interfaces()?;
            println!("{}", serde_json::to_string_pretty(&ifaces)?);
        }
        Commands::PinNics {
            mappings,
            apply,
            systemd_network_dir,
            udev_rules_path,
        } => {
            run_pin_nics(&mappings, apply, &systemd_network_dir, &udev_rules_path)?;
        }
        Commands::Render { target } => {
            let store = ConfigStore::new(&cli.config);
            let cfg = store.load_or_init()?;
            let rendered = match target {
                RenderTarget::Nftables => render_nftables_ruleset(&cfg),
                RenderTarget::Dnsmasq => render_dnsmasq_conf(&cfg)?,
                RenderTarget::Crabdns => render_crabdns_toml(&cfg)?,
            };
            print!("{rendered}");
        }
        Commands::Disks => {
            let disks = list_block_disks(Path::new("/sys/block"));
            println!("{}", serde_json::to_string_pretty(&disks)?);
        }
        Commands::Install {
            target,
            repart_dir,
            root_image,
            uki_image,
            media_dir,
            version,
            seed_config,
            dry_run,
            yes,
        } => {
            let plan = build_install_plan(&InstallOptions {
                target,
                repart_dir,
                root_image,
                uki_image,
                media_dir,
                version,
                seed_config,
            })?;
            execute_install(&plan, dry_run, yes)?;
        }
        Commands::Ota { action } => match action {
            OtaCommands::Bless {
                health_addr,
                dry_run,
            } => {
                if !dry_run {
                    verify_local_health(&health_addr).await?;
                }
                let res = bless_current_boot(dry_run).await?;
                println!("{}: {}", res.message, res.output);
                if !res.ok {
                    anyhow::bail!("boot bless failed: {}", res.output);
                }
            }
        },
    }
    Ok(())
}

async fn verify_local_health(addr: &str) -> anyhow::Result<()> {
    let mut stream = TcpStream::connect(addr).await?;
    stream
        .write_all(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf).await?;
    if !buf.contains("200 OK") {
        anyhow::bail!("crabrouterd health check at {addr} failed: {buf}");
    }
    Ok(())
}

fn run_pin_nics(
    mappings: &[String],
    apply: bool,
    systemd_network_dir: &Path,
    udev_rules_path: &Path,
) -> anyhow::Result<()> {
    let pins = if mappings.is_empty() {
        discover_physical_nic_pins(Path::new("/sys/class/net"))
    } else {
        parse_explicit_pin_mappings(mappings)?
    };
    if pins.is_empty() {
        println!("No physical Ethernet NICs discovered to pin.");
        return Ok(());
    }
    if apply {
        sync_interface_pin_files(&pins, systemd_network_dir, udev_rules_path)?;
        println!(
            "Pinned {} physical NIC(s) -> {} and {}",
            pins.len(),
            systemd_network_dir.display(),
            udev_rules_path.display()
        );
    } else {
        println!("{}", serde_json::to_string_pretty(&pins)?);
    }
    Ok(())
}

fn parse_explicit_pin_mappings(raw: &[String]) -> anyhow::Result<Vec<InterfacePin>> {
    let mut pins = Vec::with_capacity(raw.len());
    for item in raw {
        let Some((name, mac)) = item.split_once('=') else {
            anyhow::bail!("invalid --map entry '{item}' (expected <name>=<mac>)");
        };
        let name = name.trim();
        let mac = mac.trim().to_ascii_lowercase();
        if name.is_empty() || mac.len() != 17 {
            anyhow::bail!("invalid --map entry '{item}' (expected <name>=aa:bb:cc:dd:ee:ff)");
        }
        pins.push(InterfacePin {
            mac,
            name: name.to_string(),
        });
    }
    Ok(pins)
}
