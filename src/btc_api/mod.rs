use std::io::{BufReader, Cursor};

use bitcoin::{consensus::Decodable, hex::FromHex, Amount, OutPoint, Transaction, Txid};

use crate::btc_api::ordinal::Ordinal;

pub mod esplora;
pub mod ordinal;

pub mod oklink;

pub mod btc_json_rpc;

pub mod hiro;

pub mod unisat;

// hiro
// ordinal
pub trait OrdiApi {
    fn get_ordinal(&self, out_point: &OutPoint) -> anyhow::Result<Ordinal>;
    fn get_ordinals(&self, out_points: Vec<OutPoint>) -> anyhow::Result<Vec<Ordinal>>;

    fn get_inscriptions(&self, out_points: Vec<OutPoint>);
    fn get_inscription(&self, out_points: &OutPoint);

    fn get_runes(&self, out_points: Vec<OutPoint>);
    fn get_rune(&self, out_points: &OutPoint);
}

// mempool
// blockstream
// oklink
pub trait UtxoApi {
    fn get_utxos(&self, addr: &str) -> anyhow::Result<Vec<(OutPoint, Amount)>>;
    // fn get_available_utxos(
    //     &self,
    //     addr: &str,
    //     ordi_api: impl OrdiApi,
    // ) -> anyhow::Result<Vec<(OutPoint, Amount)>>;

    fn get_utxos_and_dummy(
        &self,
        addr: &str,
        dummy_amount: Amount,
    ) -> anyhow::Result<(Vec<(OutPoint, Amount)>, Vec<(OutPoint, Amount)>)>;

    fn get_confirmed_utxos(&self, addr: &str) -> anyhow::Result<Vec<(OutPoint, Amount)>>;
    fn get_unconfirmed_utxos(&self, addr: &str) -> anyhow::Result<Vec<(OutPoint, Amount)>>;
}

// btc_rpc
// mempool
// blockstream
pub trait TxApi {
    fn get_tx_hex(&self, txid: &str) -> anyhow::Result<String>;
    fn get_tx(&self, txid: &str) -> anyhow::Result<Transaction> {
        let hex_bs = Vec::from_hex(&self.get_tx_hex(txid)?)?;

        let mut tx_buf = BufReader::new(Cursor::new(hex_bs));
        Ok(Transaction::consensus_decode(&mut tx_buf)?)
    }
}

// btc_rpc
// mempool
// blockstream
// oklink
pub trait BroadcastApi {
    fn send_tx_hex(&self, hex: &str) -> anyhow::Result<Txid>;
    fn send_tx(&self, tx: &Transaction) -> anyhow::Result<Txid>;
    // fn test_mempool(&self, hex: &str) -> anyhow::Result<()>;
}
