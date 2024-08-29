use std::str::FromStr;

use anyhow::{anyhow, bail};
use bitcoin::{
    absolute::LockTime, consensus::encode, psbt::Input, transaction::Version, Address, Amount,
    FeeRate, OutPoint, Psbt, Sequence, Transaction, TxIn, TxOut,
};

use crate::{
    btc_api::esplora::Utxo, default, dummy_transaction::DummyTransaction, setting::Settings, snipe,
    utils::select_confirm,
};

pub fn send(
    settings: Settings,
    to_addr: &str,
    amount: f64,
    fee_rate: u64,
    broadcast: bool,
) -> anyhow::Result<()> {
    let wallet = settings.wallet()?;
    let pay_addr = wallet.pay_addr();
    let btc_api = settings.btc_api();
    let fee_rate = FeeRate::from_sat_per_vb(fee_rate).ok_or(anyhow!("fee_rate is invalid"))?;
    let (utxos, _) = snipe::get_utxos(&btc_api, &pay_addr.to_string())?;

    let to_addr = Address::from_str(to_addr)?.require_network(settings.network)?;
    let mut psbt = build_psbt(
        pay_addr.clone(),
        vec![(to_addr, Amount::from_btc(amount)?)],
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
        let selected = select_confirm("Please confirm");
        if selected {
            let _ = settings.broadcast(&hex)?;
        }
    }

    Ok(())
}

pub(crate) fn build_psbt(
    from_addr: Address,
    to_addr_with_amount: Vec<(Address, Amount)>,
    fee_rate: FeeRate,
    utxos: Vec<Utxo>,
    // pre_utxos: Option<Vec<Utxo>>,
) -> anyhow::Result<Psbt> {
    let mut unsigned_tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: vec![],
    };
    let mut dummy_tx = DummyTransaction::new();
    let mut need_amount = Amount::ZERO;
    // transfer
    for (to_addr, amount) in to_addr_with_amount {
        unsigned_tx.output.push(TxOut {
            value: amount,
            script_pubkey: to_addr.script_pubkey(),
        });
        dummy_tx.append_output(to_addr.script_pubkey());
        need_amount += amount;
    }
    // change
    unsigned_tx.output.push(TxOut {
        value: Amount::ZERO,
        script_pubkey: from_addr.script_pubkey(),
    });
    dummy_tx.append_output(from_addr.script_pubkey());

    let mut amount = Amount::ZERO;
    let mut ok = false;
    let mut psbt_inputs = Vec::new();
    for utxo in utxos {
        unsigned_tx.input.push(TxIn {
            previous_output: OutPoint {
                txid: utxo.txid,
                vout: utxo.vout,
            },
            script_sig: Default::default(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Default::default(),
        });
        dummy_tx.append_input(from_addr.clone(), None, None);
        psbt_inputs.push(Input {
            witness_utxo: Some({
                TxOut {
                    value: utxo.value,
                    script_pubkey: from_addr.script_pubkey(),
                }
            }),
            ..default()
        });
        amount += utxo.value;

        let network_fee = fee_rate.fee_vb(dummy_tx.vsize() as u64).unwrap();

        if let Some(unfilled) = amount.checked_sub(network_fee + need_amount) {
            unsigned_tx.output.last_mut().unwrap().value = unfilled;
            ok = true;
            break;
        }
    }

    if !ok {
        bail!("No utxo or not enough")
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
    Ok(psbt)
}
