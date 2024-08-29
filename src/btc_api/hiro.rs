use std::{fmt::Debug, num::NonZeroU32};

use anyhow::{anyhow, bail};
use axum::http::{HeaderMap, Method};
use bitcoin::{Amount, OutPoint, Txid};
use governor::{DefaultDirectRateLimiter, Quota};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{default, Print};

pub struct Client {
    http: reqwest::Client,
    rate_limiter: DefaultDirectRateLimiter,
}

trait Api {
    fn method(&self) -> Method {
        Method::GET
    }
    fn uri(&self) -> &str;
}

impl Client {
    pub fn new(api_key: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert("x-hiro-api-key", api_key.parse().unwrap());
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        let rate_limiter =
            DefaultDirectRateLimiter::direct(Quota::per_second(NonZeroU32::new(5).unwrap()));
        Self { http, rate_limiter }
    }

    // /api/v5/explorer/address/utxo

    fn endpoint() -> &'static str {
        "https://api.hiro.so"
    }

    async fn request<'a, P: Api + Serialize, D: DeserializeOwned>(
        &self,
        p: &P,
    ) -> anyhow::Result<D> {
        let _checked = self
            .rate_limiter
            .check()
            .map_err(|e| anyhow!("rate limit: {}", e))?;
        let url = format!("{}{}", Self::endpoint(), p.uri());
        let resp = match p.method() {
            Method::GET => self.http.get(url).query(p),
            Method::POST => self.http.post(url).json(p),
            _ => unreachable!(),
        }
        .send()
        .await?
        .error_for_status()?;
        let text = resp.text().await?;
        // let resp = resp.json::<D>().await?;
        text.print();
        Ok(serde_json::from_str(&text)?)
    }

    pub async fn get_inscriptions_by_outpoint(&self, outpoint: &str) -> anyhow::Result<Response> {
        self.request(&GetInscription {
            output: Some(outpoint.to_string()),
            ..default()
        })
        .await
    }

    pub async fn get_inscriptions_by_addr(&self, addr: &str) -> anyhow::Result<Response> {
        self.request(&GetInscription {
            addr: Some(addr.to_string()),
            ..default()
        })
        .await
    }
}
#[derive(Serialize, Deserialize)]
struct GetInscription {
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    addr: Option<String>,
    offset: u64,
    limit: u64,
}

impl Default for GetInscription {
    fn default() -> Self {
        Self {
            output: None,
            addr: None,
            offset: 0,
            limit: 60,
        }
    }
}

impl Api for GetInscription {
    fn uri(&self) -> &str {
        "/ordinals/v1/inscriptions"
    }
}
#[derive(Serialize, Deserialize)]
pub struct Inscription {
    pub id: String,
    pub number: i64,
    pub address: String,
    pub genesis_address: String,
    pub genesis_block_height: i64,
    pub genesis_block_hash: String,
    pub genesis_tx_id: String,
    pub genesis_fee: String,
    pub genesis_timestamp: i64,
    pub tx_id: Txid,
    pub location: String,
    pub output: OutPoint,
    pub value: Amount,
    pub offset: String,
    pub sat_ordinal: String,
    pub sat_rarity: String,
    pub sat_coinbase_height: i64,
    pub mime_type: String,
    pub content_type: String,
    pub content_length: i64,
    pub timestamp: i64,
    pub curse_type: Option<String>,
    pub recursive: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Response {
    pub limit: i64,
    pub offset: i64,
    pub total: i64,
    pub results: Vec<Inscription>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Print;

    #[tokio::test]
    async fn test_api() {
        let c = Client::new("2358d197c725f4a4610098b9b2ac78ec");
        let resp = c
            .get_inscriptions_by_addr(
                "bc1pyf5f0r5eqxer5rdrwm98grgz5tem6k8xgtnm49he2m4kjhacrsms6p6888",
            )
            .await
            .unwrap();
        resp.print();
    }
}
