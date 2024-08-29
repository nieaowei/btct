use std::{fmt::Debug, num::NonZeroU32};

use anyhow::{anyhow, bail};
use governor::{DefaultDirectRateLimiter, Quota};
use reqwest::{header::HeaderMap, Method};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};

use crate::default;

pub struct Client {
    http: reqwest::Client,
    rate_limiter: DefaultDirectRateLimiter,
}

#[derive(Debug, Deserialize)]
pub struct Response<T> {
    code: String,
    msg: String,
    data: Vec<T>,
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
        headers.insert("Ok-Access-Key", api_key.parse().unwrap());
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        let rate_limiter =
            DefaultDirectRateLimiter::direct(Quota::per_minute(NonZeroU32::new(500).unwrap()));
        Self { http, rate_limiter }
    }

    // /api/v5/explorer/address/utxo

    fn endpoint() -> &'static str {
        "https://www.oklink.com"
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
        let mut resp = resp.json::<Response<D>>().await?;
        if resp.code != "0" {
            bail!("{}", resp.msg)
        }
        Ok(resp.data.pop().ok_or(anyhow!("No data"))?)
    }

    pub async fn get_utxos(&self, addr: &str) -> anyhow::Result<UtxoResp> {
        self.request(&GetUtxoItem {
            address: addr.to_string(),
            ..default()
        })
        .await
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetUtxoItem {
    chain_short_name: String,
    address: String,
    page: u64,
    limit: u64,
}

impl Default for GetUtxoItem {
    fn default() -> Self {
        Self {
            chain_short_name: "btc".to_string(),
            address: "".to_string(),
            page: 1,
            limit: 100,
        }
    }
}

impl Api for GetUtxoItem {
    fn uri(&self) -> &str {
        "/api/v5/explorer/address/utxo"
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UtxoItem {
    pub txid: String,
    pub height: String,
    #[serde(rename = "blockTime")]
    pub block_time: String,
    pub address: String,
    #[serde(rename = "unspentAmount")]
    pub unspent_amount: String,
    pub index: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UtxoResp {
    pub page: String,
    pub limit: String,
    #[serde(rename = "totalPage")]
    pub total_page: String,
    #[serde(rename = "utxoList")]
    pub utxo_list: Vec<UtxoItem>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ProtocolType {
    Runes,
    Brc20,
    Arc20,
    Src20,
    OrdinalsNft,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTransactionDetail {
    chain_short_name: String,
    tx_id: String,
    page: u64,
    limit: u64,
    protocol_type: ProtocolType,
}

impl Api for GetTransactionDetail {
    fn uri(&self) -> &str {
        "/api/v5/explorer/inscription/transaction-detail"
    }
}

impl Default for GetTransactionDetail {
    fn default() -> Self {
        Self {
            chain_short_name: "btc".to_string(),
            tx_id: "".to_string(),
            page: 1,
            limit: 100,
            protocol_type: ProtocolType::Runes,
        }
    }
}

pub struct TransactionDetailResp {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Print;

    #[tokio::test]
    async fn test_api() {
        let c = Client::new("59ebc103-6111-471e-9610-75c8fa2fab84");
        let resp = c
            .get_utxos("bc1pyf5f0r5eqxer5rdrwm98grgz5tem6k8xgtnm49he2m4kjhacrsms6p6888")
            .await
            .unwrap();
        resp.print();
    }
}
