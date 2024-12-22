// source: https://github.com/SagerNet/sing-box/tree/dev-next/option

use experimental::Experimental;
use serde::{Deserialize, Serialize};
use shadowsocks::ShadowsocksInbound;

pub mod experimental;
pub mod shadowsocks;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SingBoxConfig {
    pub log: LogConfig,
    pub dns: DnsConfig,
    pub outbounds: Vec<Outbound>,
    pub route: RouteConfig,
    pub inbounds: Vec<ShadowsocksInbound>,
    pub experimental: Option<Experimental>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LogConfig {
    pub level: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DnsConfig {
    pub servers: Vec<DnsServer>,
    pub rules: Vec<DnsRule>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DnsServer {
    pub tag: String,
    pub address: String,
    pub strategy: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DnsRule {
    pub outbound: String,
    pub server: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Outbound {
    pub r#type: String,
    pub tag: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RouteConfig {
    pub rules: Vec<RouteRule>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RouteRule {
    pub protocol: String,
    pub outbound: String,
}
