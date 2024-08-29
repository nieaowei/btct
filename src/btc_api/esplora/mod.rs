use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::{BufReader, Cursor},
    ops::Not,
    str::FromStr,
    sync::Arc,
};

use anyhow::anyhow;
use bitcoin::{
    consensus::{encode::serialize_hex, Decodable},
    hex::FromHex,
    Amount, OutPoint,
};
pub use miniscript::bitcoin::{Network, Txid};
pub use model::*;

use crate::btc_api::{BroadcastApi, OrdiApi, TxApi, UtxoApi};

pub(crate) mod model;

#[derive(Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    network: Network,
    url: String,
}

pub fn new(network: Network) -> Client {
    Client {
        http: Default::default(),
        network,
        url: "https://mempool.space".to_string(),
    }
}

pub fn new_with_custom_url(network: Network, url: &str) -> Client {
    Client {
        http: Default::default(),
        network,
        url: url.to_string(),
    }
}

impl Client {
    pub fn new(network: Network) -> Self {
        Client {
            http: Default::default(),
            network,
            url: "https://mempool.space".to_string(),
        }
    }

    pub fn new_with_custom_url<F>(network: Network, url: &str) -> Self {
        Client {
            http: Default::default(),
            network,
            url: url.to_string(),
        }
    }

    fn base_uri(&self) -> String {
        match self.network {
            Network::Bitcoin => format!("{}/api", self.url),
            Network::Testnet => format!("{}/testnet/api", self.url),
            Network::Signet => format!("{}/signet/api", self.url),
            Network::Regtest => format!("https://127.0.0.1:8080/api"),
            _ => format!("https://127.0.0.1:8080/api"),
        }
    }

    pub fn get_utxo(&self, addr: &str) -> anyhow::Result<Vec<Utxo>> {
        let resp = self
            .http
            .get(format!("{}/address/{}/utxo", self.base_uri(), addr))
            .send()?
            .text()?;
        Ok(serde_json::from_str(&resp)?)
    }

    pub fn get_transaction(&self, tx_ix: &str) -> anyhow::Result<Transaction> {
        let text = self
            .http
            .get(format!("{}/tx/{}", self.base_uri(), tx_ix))
            .send()?
            .text()?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn get_transactions(&self, addr: &str) -> anyhow::Result<Vec<Transaction>> {
        Ok(self
            .http
            .get(format!("{}/address/{}/txs", self.base_uri(), addr))
            .send()?
            .json()?)
    }

    pub fn get_transaction_hex(&self, tx_ix: &str) -> anyhow::Result<String> {
        Ok(self
            .http
            .get(format!("{}/tx/{}/hex", self.base_uri(), tx_ix))
            .send()?
            .text()?)
    }

    pub fn get_btc_transaction(&self, tx_ix: &str) -> anyhow::Result<bitcoin::Transaction> {
        let hex_bs = Vec::from_hex(&self.get_transaction_hex(tx_ix)?)?;

        let mut tx_buf = BufReader::new(Cursor::new(hex_bs));
        Ok(bitcoin::Transaction::consensus_decode(&mut tx_buf)?)
    }

    pub fn push_transaction(&self, signed_tx: &str) -> anyhow::Result<Txid> {
        let c = self
            .http
            .post(format!("{}/tx", self.base_uri()))
            .body(signed_tx.to_string())
            .send()?
            .text()?;
        Ok(Txid::from_str(&c).map_err(|e| anyhow!(format!("{} : {}", e.to_string(), c)))?)
    }

    pub fn get_transaction_status(&self, tx_ix: &str) -> anyhow::Result<TransactionStatus> {
        let c = self
            .http
            .get(format!("{}/tx/{}/status", self.base_uri(), tx_ix))
            .send()?
            .text()?;
        Ok(serde_json::from_str(&c).map_err(|e| anyhow!(format!("{} : {}", e.to_string(), c)))?)
    }

    pub fn get_latest_block_hash(&self) -> anyhow::Result<String> {
        let c = self
            .http
            .get(format!("{}/blocks/tip/hash", self.base_uri()))
            .send()?
            .text()?;
        Ok(c)
    }

    pub fn get_latest_block_height(&self) -> anyhow::Result<u64> {
        let c = self
            .http
            .get(format!("{}/blocks/tip/height", self.base_uri()))
            .send()?
            .text()?;
        Ok(c.parse()?)
    }

    pub fn get_block_tx_ids(&self, hash: &str) -> anyhow::Result<Vec<String>> {
        let c = self
            .http
            .get(format!("{}/block/{}/txids", self.base_uri(), hash))
            .send()?
            .text()?;
        Ok(serde_json::from_str(&c)?)
    }

    pub fn get_rbf_tx(&self, tx_id: &str) -> anyhow::Result<RBFResp> {
        let c = self
            .http
            .get(format!("{}/v1/tx/{}/rbf", self.base_uri(), tx_id))
            .send()?
            .text()?;
        Ok(serde_json::from_str(&c)?)
    }
}

impl UtxoApi for Client {
    fn get_utxos(&self, addr: &str) -> anyhow::Result<Vec<(OutPoint, Amount)>> {
        let mut utxos = self
            .get_utxo(addr)?
            .iter()
            .map(|e| {
                (
                    OutPoint {
                        txid: e.txid,
                        vout: e.vout,
                    },
                    e.value,
                )
            })
            .collect::<Vec<_>>();
        utxos.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(utxos)
    }

    // fn get_available_utxos(
    //     &self,
    //     addr: &str,
    //     ordi_api: impl OrdiApi,
    // ) -> anyhow::Result<Vec<(OutPoint, Amount)>> {
    //     Ok({
    //         UtxoApi::get_utxos(self, addr)?
    //             .into_iter()
    //             .filter(|(o, _)| ordi_api.get_ordi(o).unwrap().is_none().not())
    //             .collect::<Vec<_>>()
    //     })
    // }

    fn get_utxos_and_dummy(
        &self,
        addr: &str,
        dummy_amount: Amount,
    ) -> anyhow::Result<(Vec<(OutPoint, Amount)>, Vec<(OutPoint, Amount)>)> {
        let mut utxos = self
            .get_utxo(addr)?
            .into_iter()
            .map(|e| {
                (
                    OutPoint {
                        txid: e.txid,
                        vout: e.vout,
                    },
                    e.value,
                )
            })
            .collect::<Vec<_>>();
        utxos.sort_by(|a, b| b.1.cmp(&a.1));

        Ok({
            utxos
                .into_iter()
                .partition(|(_, amount)| amount == &dummy_amount)
        })
    }

    fn get_confirmed_utxos(&self, addr: &str) -> anyhow::Result<Vec<(OutPoint, Amount)>> {
        let mut utxos = self
            .get_utxo(addr)?
            .iter()
            .filter_map(|e| {
                e.status.confirmed.then_some((
                    OutPoint {
                        txid: e.txid,
                        vout: e.vout,
                    },
                    e.value,
                ))
            })
            .collect::<Vec<_>>();
        utxos.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(utxos)
    }

    fn get_unconfirmed_utxos(&self, addr: &str) -> anyhow::Result<Vec<(OutPoint, Amount)>> {
        let mut utxos = self
            .get_utxo(addr)?
            .iter()
            .filter_map(|e| {
                e.status.confirmed.not().then_some((
                    OutPoint {
                        txid: e.txid,
                        vout: e.vout,
                    },
                    e.value,
                ))
            })
            .collect::<Vec<_>>();
        utxos.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(utxos)
    }
}

impl TxApi for Client {
    fn get_tx_hex(&self, txid: &str) -> anyhow::Result<String> {
        self.get_transaction_hex(txid)
    }

    fn get_tx(&self, txid: &str) -> anyhow::Result<bitcoin::Transaction> {
        self.get_btc_transaction(txid)
    }
}

impl BroadcastApi for Client {
    fn send_tx_hex(&self, hex: &str) -> anyhow::Result<Txid> {
        self.push_transaction(hex)
    }

    fn send_tx(&self, tx: &bitcoin::Transaction) -> anyhow::Result<Txid> {
        self.push_transaction(&serialize_hex(tx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Print;

    #[test]
    fn test_get_rbf_tx() {
        let c = Client::new(Network::Bitcoin);
        let r = c
            .get_rbf_tx("58442e4a9b1a8fa755afe15e071ba52b5131032bae2ccf2d9afabb14a12dc752")
            .unwrap();
        r.print();
    }
}
