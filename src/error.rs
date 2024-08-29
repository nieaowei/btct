use serde::Serialize;

pub(crate) enum Error {
    BtcApi(anyhow::Error),
}
