use api::v2ray_api::V2rayApi;
use clap::Parser;
use process::ProcessManager;
use std::time::Duration;
use tokio::signal;
use tokio::task;
use tracing::{error, info};

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    let args = parse_args();

    let mut fetch = api::server::ServerFetch::new(args.url, args.auth);

    let mut config = config::ConfigManager::new(fetch.clone()).await;

    let manager = ProcessManager::new(config.runtime_path.clone(), Some(true));

    if let Err(e) = manager.start().await {
        error!("Error starting sing-box: {}", e);
    } else {
        info!("sing-box started successfully");
    }

    let v2ray_api_endpoint = format!("http://{}", config.v2ray_api_endpoint);

    let mut v2ray_api = V2rayApi::new(v2ray_api_endpoint).await?;

    let _ = task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(
            config.config.as_ref().unwrap().guard_config.reporting_cycle,
        ));

        // skip the first tick
        interval.tick().await;

        loop {
            interval.tick().await;
            config.fetch().await.unwrap();

            match v2ray_api.query_all_stats(true).await {
                Ok(stats) => {
                    info!("Stats query result: {:?}", stats);
                    fetch.post_stats(stats).await.unwrap();
                    info!("Stats posted successfully!");
                }
                Err(e) => error!("Error during gRPC query: {}", e),
            }
        }
    });

    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received CTRL+C, shutting down...");
        }
    }

    if manager.is_running() {
        if let Err(e) = manager.stop().await {
            error!("Error stopping sing-box: {}", e);
        }
    }

    info!("Program exit");

    Ok(())
}
