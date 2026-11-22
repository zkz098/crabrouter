use std::path::PathBuf;

use clap::{Parser, Subcommand};
use crabrouter_core::ConfigStore;
use crabrouter_net::{read_host_interfaces, render_nftables_ruleset};
use crabrouter_services::{render_crabdns_toml, render_dnsmasq_conf};

#[derive(Debug, Parser)]
#[command(name = "crabctl", about = "CrabRouter CLI management tool")]
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
    /// Preview generated nftables, dnsmasq, or crabdns configs
    Render {
        #[arg(value_enum)]
        target: RenderTarget,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum RenderTarget {
    Nftables,
    Dnsmasq,
    Crabdns,
}

fn main() -> anyhow::Result<()> {
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
    }
    Ok(())
}
