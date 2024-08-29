use std::str::FromStr;

use anyhow::bail;
use bitcoin::{
    absolute::LockTime, consensus::encode, psbt::Input, transaction::Version, Address, Amount,
    FeeRate, OutPoint, Psbt, Sequence, Transaction, TxIn, TxOut,
};
use console_utils::input::select;

use crate::{
    btc_api::esplora,
    default,
    dummy_transaction::DummyTransaction,
    setting::Settings,
    snipe::get_utxos,
    utils::{print_table, select_confirm},
};

pub fn cancel(
    settings: Settings,
    cancel_addr: Option<String>,
    increase_fee: u64,
    postage: u64,
    dummy_utxo: u64,
    origin: bool, // todo
    peek: u64,
) -> anyhow::Result<()> {
    let wallet = settings.wallet()?;
    let btc_api = settings.btc_api();

    let pay_addr = cancel_addr.map_or(wallet.peek_addr(peek as u32), |e| {
        Address::from_str(&e).unwrap().assume_checked()
    });

    wallet.check();

    let txs = btc_api.get_transactions(&pay_addr.to_string())?;

    if txs.is_empty() {
        log::info!("No unconfirmed tx");
        return Ok(());
    }
    let mut unconfirmed_txs = txs
        .into_iter()
        .filter(|e| !e.status.confirmed)
        .collect::<Vec<_>>();

    let selected = select(
        "Please select txid: ",
        unconfirmed_txs
            .iter()
            .map(|e| e.txid.as_str())
            .collect::<Vec<_>>()
            .as_slice(),
    );

    // log::info!(
    //     "[origin tx] TotalFee: {} sat, FeeRate: {:.1} sat/vb, Size: {} vb ",
    //     snipe_pool_tx.fee,
    //     (snipe_pool_tx.fee as f64 / snipe_tx.vsize() as f64),
    //     snipe_tx.vsize(),
    // );

    let selected_tx = unconfirmed_txs.remove(selected);

    // if !select_confirm("Confirm ?") {
    //     return Ok(());
    // }

    let network_fee = Amount::from_sat(selected_tx.fee + increase_fee);

    let mut unsigned_psbt = build_psbt(
        selected_tx,
        &btc_api,
        pay_addr,
        network_fee,
        Amount::from_sat(postage),
        Amount::from_sat(dummy_utxo),
        origin,
    )?;

    log::info!("[signed psbt] {}", unsigned_psbt.serialize_hex());

    let ok = wallet.sign(&mut unsigned_psbt)?;

    if !ok {
        bail!("Sign failed")
    }
    log::info!("[signed psbt] {}", unsigned_psbt.serialize_hex());

    let signed_tx = unsigned_psbt.clone().extract_tx()?;

    let hex = encode::serialize_hex(&signed_tx);
    log::info!("[signed] {}", hex);
    print_table(&unsigned_psbt, settings.network);

    let ok = select_confirm("Confirm:");
    if ok {
        settings.broadcast(&hex)?;
    }
    Ok(())
}

pub(crate) fn build_psbt(
    unconfirmed_tx: esplora::Transaction,
    btc_api: &esplora::Client,
    addr: Address,
    fee: Amount,
    postage: Amount,
    dummy_utxo: Amount,
    origin: bool,
) -> anyhow::Result<Psbt> {
    let mut total_amount = Amount::ZERO;
    let utxos = unconfirmed_tx
        .vin
        .into_iter()
        .filter(|e| e.prevout.scriptpubkey_address == addr.to_string())
        .map(|e| {
            (
                TxIn {
                    previous_output: OutPoint {
                        txid: e.txid,
                        vout: e.vout,
                    },
                    script_sig: Default::default(),
                    sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                    witness: Default::default(),
                },
                e.prevout.value,
            )
        });

    let mut dummy_tx = DummyTransaction::new();
    let mut unsigned_tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: vec![],
    };
    let mut psbt_inputs = vec![];

    for utxo in utxos {
        unsigned_tx.input.push(utxo.0);
        dummy_tx.append_input(addr.clone(), None, None);
        psbt_inputs.push(Input {
            witness_utxo: Some({
                TxOut {
                    value: utxo.1,
                    script_pubkey: addr.script_pubkey(),
                }
            }),
            ..default()
        });

        if utxo.1 == dummy_utxo {
            unsigned_tx.output.push(TxOut {
                value: utxo.1,
                script_pubkey: addr.script_pubkey(),
            });
            dummy_tx.append_output(addr.script_pubkey());
        } else if utxo.1 == postage {
            unsigned_tx.output.push(TxOut {
                value: utxo.1,
                script_pubkey: addr.script_pubkey(),
            });
            dummy_tx.append_output(addr.script_pubkey());
        } else {
            total_amount += utxo.1;
        }
    }

    // collect
    if total_amount.to_sat() > 0 {
        unsigned_tx.output.push(TxOut {
            value: total_amount,
            script_pubkey: addr.script_pubkey(),
        });
    }

    dummy_tx.append_output(addr.script_pubkey());
    // change
    unsigned_tx.output.push(TxOut {
        value: Amount::ZERO,
        script_pubkey: addr.script_pubkey(),
    });
    dummy_tx.append_output(addr.script_pubkey());

    let (utxos, _) = get_utxos(btc_api, &addr.to_string())?;
    let mut amount = Amount::ZERO;
    let mut ok = false;
    for utxo in utxos {
        let yes = select_confirm(&format!("{}:{}:{}", utxo.txid, utxo.vout, utxo.value));
        if !yes {
            continue;
        }
        unsigned_tx.input.push(TxIn {
            previous_output: OutPoint {
                txid: utxo.txid,
                vout: utxo.vout,
            },
            script_sig: Default::default(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Default::default(),
        });

        psbt_inputs.push(Input {
            witness_utxo: Some({
                TxOut {
                    value: utxo.value,
                    script_pubkey: addr.script_pubkey(),
                }
            }),
            ..default()
        });

        dummy_tx.append_input(addr.clone(), None, None);

        amount += utxo.value;

        let network_fee = fee;

        if let Some(unfilled) = amount.checked_sub(network_fee + Amount::from_sat(1000)) {
            unsigned_tx.output.last_mut().unwrap().value = unfilled;
            ok = true;
            break;
        }
    }
    if !ok {
        unsigned_tx.output.last_mut().unwrap().value -= fee + Amount::from_sat(1000);
        //
        // bail!("No uxto or not enough")
    }

    if unsigned_tx.output.last().unwrap().value < addr.script_pubkey().dust_value() {
        unsigned_tx.output.pop();
    }

    let o_len = unsigned_tx.output.len();
    let psbt = Psbt {
        unsigned_tx,
        version: 0,
        xpub: Default::default(),
        proprietary: Default::default(),
        unknown: Default::default(),
        inputs: psbt_inputs,
        outputs: vec![default(); o_len],
    };
    // if total_amount <= network_fee + Amount::from_sat(1000) {
    //     // 余额不够
    // } else {
    //     for out in psbt.unsigned_tx.output {}
    // }
    Ok(psbt)
}
