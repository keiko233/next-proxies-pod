use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowsocksInbound {
    pub r#type: String,
    pub tag: String,
    pub listen: String,
    pub listen_port: u16,
    pub network: Option<String>,
    pub method: String,
    pub password: Option<String>,
    pub users: Option<Vec<ShadowsocksUser>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowsocksUser {
    pub name: String,
    pub password: String,
}
