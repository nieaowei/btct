use std::{thread, time::Duration};

use anyhow::anyhow;

use crate::{audio, setting::Settings};

pub fn monitor(settings: &Settings, txid: &str, interval: u64) -> anyhow::Result<bool> {
    let rpc_api = settings
        .rpc_api()
        .ok_or(anyhow!("Please setting [rpc_url]"))?;
    thread::sleep(Duration::from_secs(5));

    loop {
        match rpc_api.get_tx(txid) {
            Ok(tx) => {
                if tx.confirmations.unwrap_or(0) > 0 {
                    log::info!("[{}] Tx has confirmed", tx.txid);
                    audio::play_confirmed();
                    return Ok(true);
                }
            }
            Err(err) => {
                log::error!("[monitor] {}", err);
                log::info!("[{}]Tx has replaced", txid);
                audio::play_replaced();
                return Ok(false);
            }
        }
        log::info!("[monitor] Running...");
        thread::sleep(Duration::from_secs(interval));
    }
    Ok(false)
}
