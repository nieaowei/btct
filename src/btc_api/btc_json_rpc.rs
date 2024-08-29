use std::{fmt::format, num::NonZeroU32};

use anyhow::{anyhow, bail};
use axum::http::{HeaderMap, Method};
use bitcoin::{Transaction, Txid};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};

use crate::{btc_api::TxApi, Print};

pub struct Client {
    http: reqwest::blocking::Client,
    rate_limiter: DefaultDirectRateLimiter,
    endpoint: String,
}

trait Api: Serialize {
    fn method_name(&self) -> &'static str;
}

impl Client {
    pub fn new(endpoint: &str, rps: u32) -> Self {
        let headers = HeaderMap::new();
        let http = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        let rate_limiter =
            DefaultDirectRateLimiter::direct(Quota::per_second(NonZeroU32::new(rps).unwrap()));
        Self {
            http,
            rate_limiter,
            endpoint: endpoint.to_string(),
        }
    }

    fn request<P: Api, D>(&self, p: &P) -> anyhow::Result<D>
    where
        D: DeserializeOwned,
    {
        let params = serde_json::to_value(p)?
            .as_object()
            .ok_or(anyhow!("not object"))?
            .into_iter()
            .map(|e| e.1.clone())
            .collect::<Vec<_>>();

        let resp = self
            .http
            .post(format!("{}", self.endpoint))
            .json(&Request::new(p.method_name(), params))
            .send()?
            .error_for_status()?
            .json::<Response<D>>()?;
        if let Some(err) = resp.error {
            bail!("{:?}", err)
        }
        Ok(resp.result)
    }

    pub fn get_raw_tx(&self, tx_id: &str) -> anyhow::Result<String> {
        let param = GetRawTx {
            txid: tx_id.to_string(),
            verbose: None,
            blockhash: None,
        };
        self.request(&param)
    }

    pub fn get_tx(&self, tx_id: &str) -> anyhow::Result<Tx> {
        let params = GetRawTx {
            txid: tx_id.to_string(),
            verbose: Some(true),
            blockhash: None,
        };
        self.request(&params)
    }
    pub fn send_raw_tx(&self, hex: &str) -> anyhow::Result<String> {
        let params = SendTxParams {
            hexstring: hex.to_string(),
            maxfeerate: None,
        };
        self.request(&params)
    }
}

#[derive(Serialize, Deserialize)]
struct Request {
    jsonrpc: String,
    pub method: String,
    pub params: Vec<serde_json::Value>,
    id: String,
}

impl Request {
    pub fn new(method: &str, params: Vec<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: params,
            id: "".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Response<T> {
    pub error: Option<String>,
    pub id: String,
    pub result: T,
}

#[derive(Serialize, Deserialize)]
struct GetRawTx {
    txid: String,
    verbose: Option<bool>,
    blockhash: Option<String>,
}

impl Api for GetRawTx {
    fn method_name(&self) -> &'static str {
        "getrawtransaction"
    }
}

#[derive(Serialize, Deserialize)]
struct ScriptPubKey {
    pub asm: String,
    pub desc: String,
    pub hex: String,
    pub address: Option<String>,
    #[serde(rename = "type")]
    pub r#type: String,
}

#[derive(Serialize, Deserialize)]
struct VOut {
    pub value: f64,
    pub n: i64,
    #[serde(rename = "scriptPubKey")]
    pub script_pub_key: ScriptPubKey,
}

#[derive(Serialize, Deserialize)]
struct ScriptSig {
    pub asm: String,
    pub hex: String,
}

#[derive(Serialize, Deserialize)]
struct VIn {
    pub txid: String,
    pub vout: i64,
    #[serde(rename = "scriptSig")]
    pub script_sig: ScriptSig,
    pub txinwitness: Vec<String>,
    pub sequence: i64,
}

#[derive(Serialize, Deserialize)]
pub struct Tx {
    pub txid: Txid,
    pub hash: String,
    pub version: i64,
    pub size: i64,
    pub vsize: i64,
    pub weight: i64,
    pub locktime: i64,
    pub vin: Vec<VIn>,
    pub vout: Vec<VOut>,
    pub hex: String,
    pub blockhash: Option<String>,
    pub confirmations: Option<i64>,
    pub time: Option<i64>,
    pub blocktime: Option<i64>,
}

#[derive(Serialize, Deserialize)]
struct SendTxParams {
    hexstring: String,
    maxfeerate: Option<f64>,
}

impl Api for SendTxParams {
    fn method_name(&self) -> &'static str {
        "sendrawtransaction"
    }
}

impl TxApi for Client {
    fn get_tx_hex(&self, txid: &str) -> anyhow::Result<String> {
        self.get_raw_tx(txid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Print;

    #[test]
    fn test_api() {
        let c = Client::new(
            "https://go.getblock.io/219167dbfc504ab5b2d63863fc74a1c7",
            60,
        );

        c.get_raw_tx("817721ce6aecb6bc4a77326d16313261654fa3dd2f262a14f64999800bd209eb")
            .unwrap()
            .print();
        c.get_tx("215b9f6054e6fd88fe5c54e128f35ad5904b866368aa4d55ef2acb5302e01d81")
            .unwrap()
            .print();
    }
}
