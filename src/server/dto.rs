use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub(super) struct GetSnipePrams {
    pub(super) txid: String,
    pub(super) increase_fee: u64,
}

#[derive(Serialize)]
pub(super) struct GetSnipeResp {
    pub(super) hex: String,
}
