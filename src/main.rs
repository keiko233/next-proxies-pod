use api::v2ray_api::V2rayApi;
use clap::Parser;
use config::FetchStatus;
use process::ProcessManager;
use std::{sync::Arc, time::Duration};
use tokio::signal;
use tokio::sync::mpsc;
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

#[derive(Debug)]
enum ReportingTask {
    FetchConfig,

    PostStats,

    ReloadConfig,
}

impl ReportingTask {
    /// Handle the task
    async fn handle(
        self,
        config: &mut config::ConfigManager,
        fetch: &mut api::server::ServerFetch,
        v2ray_api: &mut V2rayApi,
        manager: &ProcessManager,
    ) {
        match self {
            ReportingTask::FetchConfig => {
                if let Err(e) = config.fetch().await {
                    error!("Error fetching config: {}", e);
                } else {
                    info!("Fetch config done");
                }
            }
            ReportingTask::PostStats => match v2ray_api.query_all_stats(true).await {
                Ok(stats) => {
                    debug!("Stats query result: {:?}", stats);
                    if let Err(e) = fetch.post_stats(stats).await {
                        error!("Error posting stats: {}", e);
                    } else {
                        info!("Stats posted successfully!");
                    }
                }
                Err(e) => error!("Error during gRPC query: {}", e),
            },
            ReportingTask::ReloadConfig => {
                if matches!(config.fetch_status, Some(FetchStatus::Updated)) {
                    if let Err(e) = manager.reload().await {
                        error!("Error reloading sing-box: {}", e);
                    } else {
                        info!("Reloaded sing-box successfully");
                    }
                }
            }
        }
    }
}

/// Producer that generates tasks and sends them to the queue
async fn reporting_tasks_producer(tx: mpsc::Sender<ReportingTask>, interval_secs: u64) {
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

    // skip the first tick
    interval.tick().await;

    loop {
        interval.tick().await;
        if let Err(e) = tx.send(ReportingTask::FetchConfig).await {
            error!("Error sending FetchConfig task: {}", e);
            break;
        }

        if let Err(e) = tx.send(ReportingTask::PostStats).await {
            error!("Error sending PostStats task: {}", e);
            break;
        }

        if let Err(e) = tx.send(ReportingTask::ReloadConfig).await {
            error!("Error sending ReloadConfig task: {}", e);
            break;
        }
    }
}

/// Consumer that receives tasks from the queue and executes them
async fn reporting_tasks_consumer(
    mut rx: mpsc::Receiver<ReportingTask>,
    mut config: config::ConfigManager,
    mut fetch: api::server::ServerFetch,
    mut v2ray_api: V2rayApi,
    manager: Arc<ProcessManager>,
) {
    while let Some(task) = rx.recv().await {
        task.handle(&mut config, &mut fetch, &mut v2ray_api, &manager)
            .await;
    }
}

/// Wrap producer and consumer and run concurrently
async fn spawn_reporting_tasks(
    config: config::ConfigManager,
    fetch: api::server::ServerFetch,
    v2ray_api: V2rayApi,
    manager: Arc<ProcessManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let interval_secs = config.config.as_ref().unwrap().guard_config.reporting_cycle;
    info!("Reporting interval: {}s", interval_secs);

    // Create a mpsc channel
    let (tx, rx) = mpsc::channel::<ReportingTask>(100);

    // Start the consumer (task handler)
    let consumer_handle = task::spawn(reporting_tasks_consumer(
        rx,
        config,
        fetch,
        v2ray_api,
        Arc::clone(&manager),
    ));

    // Start the producer (task generator)
    let producer_handle = task::spawn(reporting_tasks_producer(tx, interval_secs));

    // Wait for either the producer or consumer to finish
    tokio::select! {
        _ = consumer_handle => {
            error!("Consumer ended unexpectedly.");
        },
        _ = producer_handle => {
            error!("Producer ended unexpectedly.");
        },
    }

    Ok(())
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

    let manager_arc = Arc::new(manager);

    // Initialize V2Ray API
    let v2ray_api_endpoint = format!("http://{}", config.v2ray_api_endpoint);
    let v2ray_api = V2rayApi::new(v2ray_api_endpoint).await?;

    // Run reporting tasks concurrently (producer + consumer)
    let reporting_handle = spawn_reporting_tasks(
        config,
        fetch,
        v2ray_api,
        Arc::clone(&manager_arc),
    );

    // Wait for shutdown signal
    tokio::select! {
        res = reporting_handle => {
            if let Err(e) = res {
                error!("Reporting task error: {}", e);
            }
        }
        _ = signal::ctrl_c() => {
            info!("Received CTRL+C, shutting down...");
        }
    }

    // Cleanup
    shutdown_manager(&manager_arc).await;
    info!("Program exit");

    Ok(())
}
