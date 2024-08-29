use miniscript::bitcoin::{transaction::Version, Amount, Sequence, Txid};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Status {
    pub confirmed: bool,
    pub block_height: Option<u64>,
    pub block_hash: Option<String>,
    pub block_time: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct Utxo {
    pub txid: Txid,
    pub vout: u32,
    pub status: Status,
    pub value: Amount,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Prevout {
    pub scriptpubkey: String,
    pub scriptpubkey_asm: String,
    pub scriptpubkey_type: String,
    #[serde(default)]
    pub scriptpubkey_address: String,
    pub value: Amount,
}

#[derive(Serialize, Deserialize)]
pub struct Input {
    pub txid: Txid,
    pub vout: u32,
    pub prevout: Prevout,
    pub scriptsig: String,
    pub scriptsig_asm: String,
    pub witness: Option<Vec<String>>,
    pub is_coinbase: bool,
    pub sequence: Sequence,
}

#[derive(Serialize, Deserialize)]
pub struct Transaction {
    pub txid: String,
    pub version: Version,
    pub locktime: u32,
    pub vin: Vec<Input>,
    pub vout: Vec<Prevout>,
    pub size: u64,
    pub weight: u64,
    pub sigops: u64,
    pub fee: u64,
    pub status: Status,
}

#[derive(Serialize, Deserialize)]
pub struct Block {}

// {
// confirmed: true,
// block_height: 363348,
// block_hash: "0000000000000000139385d7aa78ffb45469e0c715b8d6ea6cb2ffa98acc7171",
// block_time: 1435754650
// }
#[derive(Serialize, Deserialize)]
pub struct TransactionStatus {
    pub confirmed: bool,
    pub block_height: u64,
    pub block_hash: String,
    pub block_time: u64,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ReplaceTx {
    pub tx: CurrentTx,
    pub time: i64,
    #[serde(rename = "fullRbf", default)]
    pub full_rbf: bool,
    pub replaces: Vec<ReplaceTx>,
    pub interval: i64,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct CurrentTx {
    pub txid: String,
    pub fee: i64,
    pub vsize: f64,
    pub value: i64,
    pub rate: f64,
    pub time: i64,
    pub rbf: bool,
    #[serde(rename = "fullRbf", default)]
    pub full_rbf: bool,
    #[serde(default)]
    pub mined: bool,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Replacements {
    pub tx: CurrentTx,
    pub time: i64,
    #[serde(rename = "fullRbf", default)]
    pub full_rbf: bool,
    pub replaces: Vec<ReplaceTx>,
    #[serde(default)]
    pub mined: bool,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct RBFResp {
    pub replacements: Option<Replacements>,
    pub replaces: Option<Vec<String>>,
}
