use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Experimental {
    pub v2ray_api: V2rayApi,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct V2rayApi {
    pub listen: String,
    pub stats: V2rayApiStats,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct V2rayApiStats {
    pub enabled: bool,
    pub inbounds: Vec<String>,
    pub outbounds: Vec<String>,
    pub users: Vec<String>,
}
