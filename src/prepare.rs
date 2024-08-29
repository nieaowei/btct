use bitcoin::FeeRate;

use crate::{constant::DUMMY_UTXO, send, setting::Settings};

pub(crate) fn prepare(settings: Settings, fee_rate: u64, number: u64) -> anyhow::Result<()> {
    let mut wallet = settings.wallet()?;
    let btc_api = settings.btc_api();
    let pay_addr = wallet.pay_addr();
    let fee_rate = FeeRate::from_sat_per_vb_unchecked(fee_rate);
    let utxos = btc_api.get_utxo(&pay_addr.to_string())?;

    let psbt = send::build_psbt(
        pay_addr.clone(),
        vec![(pay_addr.clone(), DUMMY_UTXO); number as usize],
        fee_rate,
        utxos,
    )?;

    Ok(())
}
//
// fn build_psbt(
//     utxos: Vec<Utxo>,
//     fee_rate: FeeRate,
//     pay_addr: Address,
//     number: u64,
// ) -> anyhow::Result<()> {
//     let mut unsigned_tx = Transaction {
//         version: Version::TWO,
//         lock_time: LockTime::ZERO,
//         input: vec![],
//         output: vec![],
//     };
//     let mut dummy_tx = DummyTransaction::new();
//
//     let mut need_amount = Amount::ZERO;
//     for _ in 0..number {
//         unsigned_tx.output.push(TxOut {
//             value: DUMMY_UTXO,
//             script_pubkey: pay_addr.script_pubkey(),
//         });
//         dummy_tx.append_output(pay_addr.script_pubkey());
//
//         need_amount += DUMMY_UTXO;
//     }
//
//     // change
//     unsigned_tx.output.push(TxOut {
//         value: Amount::ZERO,
//         script_pubkey: pay_addr.script_pubkey(),
//     });
//     dummy_tx.append_output(pay_addr.script_pubkey());
//
//     let mut amount = Amount::ZERO;
//     let mut ok = false;
//     for utxo in utxos {
//         unsigned_tx.input.push(TxIn {
//             previous_output: OutPoint {
//                 txid: utxo.txid,
//                 vout: utxo.vout,
//             },
//             script_sig: Default::default(),
//             sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
//             witness: Default::default(),
//         });
//         dummy_tx.append_input(pay_addr.clone(), None, None);
//         amount += utxo.value;
//
//         let network_fee = fee_rate.fee_vb(dummy_tx.vsize() as u64).unwrap();
//
//         if let Some(unfilled) = amount.checked_sub(network_fee + need_amount) {
//             unsigned_tx.output.last_mut().unwrap().value = unfilled;
//             ok = true;
//             break;
//         }
//     }
//     if !ok {
//         bail!("No utxo or not enough")
//     }
//
//     Ok(())
// }
