use api::v2ray_api::V2rayApi;
use clap::Parser;
use process::ProcessManager;
use std::time::Duration;
use tokio::signal;
use tokio::task;
use tracing::{debug, error, info};

mod api;
mod config;
mod process;

#[derive(Parser)]
#[command(name = "next-proxies-pod")]
struct Args {
    #[arg(long)]
    url: String,

    #[arg(long)]
    auth: String,
}

fn parse_args() -> Args {
    Args::parse()
}

async fn setup_process_manager(
    config: &config::ConfigManager,
) -> Result<ProcessManager, Box<dyn std::error::Error + Send + Sync>> {
    let manager = ProcessManager::new(config.runtime_path.clone(), Some(true));
    if let Err(e) = manager.start().await {
        error!("Error starting sing-box: {}", e);
        return Err(e.into());
    }
    info!("sing-box started successfully");
    Ok(manager)
}

async fn stats_reporting_task(
    mut config: config::ConfigManager,
    mut fetch: api::server::ServerFetch,
    mut v2ray_api: V2rayApi,
) {
    let interval_secs = config.config.as_ref().unwrap().guard_config.reporting_cycle;
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

    // skip the first tick
    interval.tick().await;

    loop {
        interval.tick().await;
        if let Err(e) = config.fetch().await {
            error!("Error fetching config: {}", e);
            continue;
        }

        match v2ray_api.query_all_stats(true).await {
            Ok(stats) => {
                debug!("Stats query result: {:?}", stats);
                if let Err(e) = fetch.post_stats(stats).await {
                    error!("Error posting stats: {}", e);
                    continue;
                }
                info!("Stats posted successfully!");
            }
            Err(e) => error!("Error during gRPC query: {}", e),
        }
    }
}

async fn shutdown_manager(manager: &ProcessManager) {
    if manager.is_running() {
        if let Err(e) = manager.stop().await {
            error!("Error stopping sing-box: {}", e);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    // Initialize components
    let args = parse_args();
    let fetch = api::server::ServerFetch::new(args.url, args.auth);
    let config = config::ConfigManager::new(fetch.clone()).await;

    // Setup process manager
    let manager = setup_process_manager(&config).await?;

    // Initialize V2Ray API
    let v2ray_api_endpoint = format!("http://{}", config.v2ray_api_endpoint);
    let v2ray_api = V2rayApi::new(v2ray_api_endpoint).await?;

    // Spawn stats reporting task
    let _ = task::spawn(stats_reporting_task(config, fetch, v2ray_api));

    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received CTRL+C, shutting down...");
        }
    }

    // Cleanup
    shutdown_manager(&manager).await;
    info!("Program exit");

    Ok(())
}
