mod cert;
mod hub;
mod web;

use std::{
    net::{SocketAddr, ToSocketAddrs},
    path::PathBuf,
};

use anyhow::{Context, Result};
use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use hub::{AppState, HubState};

#[derive(Parser)]
#[command(name = "near", version, about = "LAN-only file transfer hub")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = 53317)]
    port: u16,

    /// Address to bind
    #[arg(short = 'H', long, default_value = "0.0.0.0")]
    host: String,

    /// Directory for the generated certificate authority
    #[arg(short, long)]
    cert_dir: Option<PathBuf>,

    /// Serve a built site from this directory instead of the bundled page
    #[arg(short, long)]
    web_dir: Option<PathBuf>,

    /// Print the CA path and fingerprint, then exit
    #[arg(long)]
    print_ca: bool,
}

fn default_cert_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join("near")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let dir = args.cert_dir.unwrap_or_else(default_cert_dir);
    let certs = cert::ensure(&dir)?;

    println!("near {}", env!("CARGO_PKG_VERSION"));
    println!("ca: {}", certs.ca_path.display());
    println!("ca sha256: {}", certs.fingerprint);

    if args.print_ca {
        return Ok(());
    }

    let tls = RustlsConfig::from_pem_file(&certs.cert_path, &certs.key_path)
        .await
        .context("failed to load the local certificate")?;

    let address = format!("{}:{}", args.host, args.port)
        .to_socket_addrs()?
        .next()
        .context("could not resolve the listen address")?;
    let address = SocketAddr::from((address.ip(), args.port));

    println!("open on this lan:");
    for ip in &certs.addresses {
        println!("  https://{}:{}/near", ip, args.port);
    }
    println!("  https://near.local:{}/near", args.port);

    let app = hub::router(AppState {
        hub: HubState::new(),
        web_root: args.web_dir,
    });

    println!("listening on {}", address);
    axum_server::bind_rustls(address, tls)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
