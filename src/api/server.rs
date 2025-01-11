use super::v2ray_api::StatsFormatResponse;
use crate::config::ConfigResponse;
use reqwest::{Client, header::HeaderMap};
use std::error::Error;
use tracing::info;

#[derive(Debug, Clone)]
pub struct ServerFetch {
    pub url: String,
    headers: HeaderMap,
    client: Client,
}

impl ServerFetch {
    pub fn new(url: String, authorization: String) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert("X-Proxy-Authorization", authorization.parse().unwrap());

        let client = Client::new();

        Self {
            url,
            headers,
            client,
        }
    }

    pub async fn get_config(&mut self) -> Result<ConfigResponse, Box<dyn Error>> {
        let response = self
            .client
            .get(&self.url)
            .headers(self.headers.clone())
            .send()
            .await?;

        match response.status().is_success() {
            true => {
                let body = response.text().await?;
                Ok(serde_json::from_str(&body)?)
            }
            false => Err("Error fetching config".into()),
        }
    }

    pub async fn post_stats(&mut self, stats: StatsFormatResponse) -> Result<(), Box<dyn Error>> {
        let response = self
            .client
            .post(&self.url)
            .headers(self.headers.clone())
            .body(serde_json::to_string(&stats)?)
            .send()
            .await?;

        match response.status().is_success() {
            true => {
                info!("Stats response: {:?}", response.text().await?);
                Ok(())
            }
            false => Err("Error posting stats".into()),
        }
    }
}
