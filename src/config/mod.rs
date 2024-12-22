use serde::{Deserialize, Serialize};
use sing_box::{
    SingBoxConfig,
    experimental::{Experimental, V2rayApi, V2rayApiStats},
};
use std::{error::Error, path::PathBuf};
use temp_dir::TempDir;
use tracing::{error, info};

use crate::api::server::ServerFetch;

mod sing_box;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConfigResponse {
    pub runtime: SingBoxConfig,

    #[serde(rename = "guardConfig")]
    pub guard_config: GuardConfig,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GuardConfig {
    pub reporting_cycle: u64,
}

pub struct ConfigManager {
    pub fetch: ServerFetch,

    pub config: Option<ConfigResponse>,

    #[allow(dead_code)]
    temp_dir: TempDir,

    pub runtime_path: PathBuf,

    pub v2ray_api_endpoint: String,
}

impl ConfigManager {
    pub async fn new(fetch: ServerFetch) -> Self {
        let temp_dir = TempDir::new().unwrap();

        let runtime_path = temp_dir.child("singbox-runtime.json");

        let port = portpicker::pick_unused_port().expect("No ports free");

        let mut config = Self {
            fetch,
            config: None,
            temp_dir,
            runtime_path,
            v2ray_api_endpoint: format!("localhost:{}", port),
        };
        config.fetch().await.unwrap();

        config
    }

    pub async fn fetch(&mut self) -> Result<(), Box<dyn Error>> {
        let response = self.fetch.get_config().await?;

        self.config = Some(response);

        let _ = self.prepare();

        let runtime_str = serde_json::to_string(&self.config.clone().unwrap().runtime)?;

        match std::fs::write(&self.runtime_path, runtime_str) {
            Ok(_) => {
                info!(
                    "Runtime configuration successful saved to: {}",
                    self.runtime_path.display()
                );
            }
            Err(e) => {
                error!("Failed to update runtime configuration: {}", e);
            }
        }

        Ok(())
    }

    fn prepare(&mut self) -> Result<(), Box<dyn Error>> {
        if self.config.is_none() {
            return Err("Configuration not fetched".into());
        }

        let runtime = &mut self.config.as_mut().unwrap().runtime;

        // prepare v2ray api
        runtime.experimental = Some(Experimental {
            v2ray_api: V2rayApi {
                listen: self.v2ray_api_endpoint.to_string(),
                stats: V2rayApiStats {
                    enabled: true,
                    inbounds: runtime.inbounds.iter().map(|i| i.tag.clone()).collect(),
                    outbounds: runtime.outbounds.iter().map(|o| o.tag.clone()).collect(),
                    users: runtime
                        .inbounds
                        .iter()
                        .flat_map(|i| {
                            i.users
                                .iter()
                                .flat_map(|u| u.iter().map(|user| user.name.clone()))
                        })
                        .collect(),
                },
            },
        });

        Ok(())
    }
}

mod tests {
    use super::*;

    #[allow(dead_code)]
    async fn setup_test_config() -> ConfigManager {
        let fetch = ServerFetch::new(
            "http://localhost:3000/api/provider/proxy?id=cm4ivp4i50004usi8fkq2uffq".to_string(),
            "OmutiAkm7eW2W1m3XradC1/rO41JzoFk0Vt6f7mFvFQEUJMovBGOIv+3Hr9fB3yVwKhJqSk=".to_string(),
        );

        ConfigManager::new(fetch).await
    }

    #[tokio::test]
    async fn test_config_fetch() {
        let config = setup_test_config().await;

        println!("{:?}", config.config.as_ref().unwrap());

        assert!(config.config.is_some());
    }

    #[tokio::test]
    async fn test_config_file() {
        let config = setup_test_config().await;

        let runtime = std::fs::read_to_string(&config.runtime_path).unwrap();

        println!("save path: {:?}", config.runtime_path);
        println!("runtime config: {:?}", runtime);

        assert!(!runtime.is_empty());
    }
}
