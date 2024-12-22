use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tonic::transport::Channel;
use v2rayapi::QueryStatsRequest;
use v2rayapi::stats_service_client::StatsServiceClient;

pub mod v2rayapi {
    include!("../proto-gen/v2ray.core.app.stats.command.rs");
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StatsFormatResponse {
    server: Vec<ServerStats>,
    user: Vec<UserStats>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ServerStats {
    pub id: String,
    pub uplink: u64,
    pub download: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct UserStats {
    pub user: String,
    pub uplink: u64,
    pub download: u64,
}

#[derive(Clone, Debug)]
pub struct V2rayApi {
    client: Option<StatsServiceClient<Channel>>,
}

impl V2rayApi {
    pub async fn new(url: impl Into<String>) -> Result<Self> {
        let client = match StatsServiceClient::connect(url.into()).await {
            Ok(client) => client,
            Err(e) => return Err(e.into()),
        };

        Ok(Self {
            client: Some(client),
        })
    }

    pub async fn query_all_stats(&mut self, reset: bool) -> Result<StatsFormatResponse> {
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Client not initialized"))?;

        let req = tonic::Request::new(QueryStatsRequest {
            pattern: String::new(),
            patterns: vec!["traffic".to_string()],
            regexp: false,
            reset,
        });

        let res = client.query_stats(req).await?;
        let stats_list = &res.get_ref().stat;

        let server_regex = Regex::new(r"^inbound>>>[^>]+>>>traffic>>>(uplink|downlink)$").unwrap();
        let user_regex = Regex::new(r"^user>>>[^>]+>>>traffic>>>(uplink|downlink)$").unwrap();

        let mut server_stats: Vec<ServerStats> = Vec::new();
        let mut user_stats: Vec<UserStats> = Vec::new();

        for stat in stats_list {
            let name = &stat.name;
            let value = stat.value;

            if let Some(captures) = server_regex.captures(name) {
                let id = name.split(">>>").nth(1).unwrap_or_default().to_string();
                let is_uplink = captures.get(1).map_or(false, |m| m.as_str() == "uplink");

                if let Some(server) = server_stats.iter_mut().find(|s| s.id == id) {
                    if is_uplink {
                        server.uplink = value as u64;
                    } else {
                        server.download = value as u64;
                    }
                } else {
                    let mut new_stat = ServerStats {
                        id,
                        uplink: 0,
                        download: 0,
                    };
                    if is_uplink {
                        new_stat.uplink = value as u64;
                    } else {
                        new_stat.download = value as u64;
                    }
                    server_stats.push(new_stat);
                }
            } else if let Some(captures) = user_regex.captures(name) {
                let user = name.split(">>>").nth(1).unwrap_or_default().to_string();
                let is_uplink = captures.get(1).map_or(false, |m| m.as_str() == "uplink");

                if let Some(user_stat) = user_stats.iter_mut().find(|u| u.user == user) {
                    if is_uplink {
                        user_stat.uplink = value as u64;
                    } else {
                        user_stat.download = value as u64;
                    }
                } else {
                    let mut new_stat = UserStats {
                        user,
                        uplink: 0,
                        download: 0,
                    };
                    if is_uplink {
                        new_stat.uplink = value as u64;
                    } else {
                        new_stat.download = value as u64;
                    }
                    user_stats.push(new_stat);
                }
            }
        }

        Ok(StatsFormatResponse {
            server: server_stats,
            user: user_stats,
        })
    }
}
