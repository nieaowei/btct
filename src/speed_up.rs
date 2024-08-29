use anyhow::bail;
use bitcoin::{consensus::encode, FeeRate, Weight};
use console_utils::input::select;

use crate::{send, setting::Settings, snipe::get_utxos, utils::select_confirm};

pub fn speed_up(
    settings: Settings,
    txid: &str,
    increase_rate: u64,
    broadcast: bool,
) -> anyhow::Result<()> {
    let btc_api = settings.btc_api();
    let wallet = settings.wallet()?;

    let pay_addr = wallet.pay_addr();
    let tx = btc_api.get_transaction(txid)?;
    let fee_rate =
        FeeRate::from_sat_per_vb(tx.fee / Weight::from_wu(tx.weight).to_vbytes_ceil()).unwrap();

    let options = tx
        .vout
        .iter()
        .enumerate()
        .filter(|(i, e)| e.scriptpubkey_address == pay_addr.to_string())
        .map(|(i, e)| (i, e, format!("{} {}", e.scriptpubkey_address, e.value)))
        .collect::<Vec<_>>();

    let selected = select(
        "Please select utxo",
        &options
            .iter()
            .map(|((_, _, display))| display.as_str())
            .collect::<Vec<_>>(),
    );

    let selected = options[selected].0;

    let (utxos, _) = get_utxos(&btc_api, &pay_addr.to_string())?;
    let mut psbt = send::build_psbt(
        pay_addr.clone(),
        vec![(pay_addr.clone(), tx.vout[selected].value)],
        fee_rate,
        utxos,
    )?;

    let ok = wallet.sign(&mut psbt)?;
    if !ok {
        bail!("Sign failed")
    }

    let signed_tx = psbt.extract_tx()?;

    let hex = encode::serialize_hex(&signed_tx);

    if broadcast {
        if select_confirm("") {
            settings.broadcast(&hex)?;
        }
    }

    Ok(())
}
