use std::str::FromStr;

use bitcoin::{Address, Network, Psbt, TxIn, TxOut};
use console_utils::input::select;

use crate::btc_api::{esplora::Prevout, ordinal::Ordinal};

pub(crate) fn print_snipe_table(
    signed_psbt: &Psbt,
    ordinals: Vec<(usize, Ordinal, (TxIn, Prevout), TxOut)>, // real_index, ordi , (input, input_tx) , output
    network: Network,
) {
    let mut inputs = signed_psbt
        .unsigned_tx
        .input
        .iter()
        .enumerate()
        .map(|(txin_index, txin)| {
            let o = ordinals
                .to_owned()
                .into_iter()
                .find(|(_, ordi, _, _)| match ordi {
                    Ordinal::None => false,
                    Ordinal::Inscription { out_point, .. } => out_point == &txin.previous_output,
                    Ordinal::Rune { out_point, .. } => out_point == &txin.previous_output,
                });
            match o {
                None => {
                    format!(
                        "{}\n{}\n{}",
                        Address::from_script(
                            signed_psbt.inputs[txin_index]
                                .witness_utxo
                                .as_ref()
                                .unwrap()
                                .script_pubkey
                                .as_script(),
                            network
                        )
                        .unwrap(),
                        signed_psbt.inputs[txin_index]
                            .witness_utxo
                            .as_ref()
                            .unwrap()
                            .value,
                        txin.previous_output
                    )
                }
                Some((i, ordinal, (txin, prevout), txout)) => match &ordinal {
                    Ordinal::None => {
                        format!(
                            "{}\n{}\n{}",
                            prevout.scriptpubkey_address,
                            ordinal.display(),
                            txin.previous_output,
                        )
                    }
                    Ordinal::Inscription { .. } => {
                        format!(
                            "{}\n{}\n{}",
                            prevout.scriptpubkey_address,
                            ordinal.display(),
                            txin.previous_output,
                        )
                    }
                    Ordinal::Rune { number, .. } => {
                        format!(
                            "{}\n{} {:.2} sat/unit\n{}",
                            prevout.scriptpubkey_address,
                            ordinal.display(),
                            txout.value.to_sat() as f64 / *number as f64,
                            txin.previous_output,
                        )
                    }
                },
            }
        })
        .collect::<Vec<_>>();

    let mut outputs = signed_psbt
        .unsigned_tx
        .output
        .iter()
        .map(
            |e| match Address::from_script(e.script_pubkey.as_script(), network) {
                Ok(addr) => {
                    format!("{}\n{}", addr, e.value)
                }
                Err(_) => {
                    format!("{}", e.script_pubkey.to_asm_string())
                }
            },
        )
        .collect::<Vec<_>>();

    if inputs.len() < signed_psbt.unsigned_tx.output.len() {
        inputs.resize(signed_psbt.unsigned_tx.output.len(), "".to_string());
    } else {
        outputs.resize(inputs.len(), "".to_string());
    }

    let rows = inputs
        .into_iter()
        .zip(outputs.into_iter())
        .map(|e| vec![e.0, e.1])
        .collect::<Vec<_>>();
    let mut table = comfy_table::Table::new();

    table.set_header(vec!["Inputs", "Outputs"]);

    for row in rows {
        table.add_row(row);
    }
    println!("{}", table);
}

pub(crate) fn print_table(psbt: &Psbt, network: Network) {
    let mut inputs = psbt
        .unsigned_tx
        .input
        .iter()
        .map(|e| format!("{}", e.previous_output))
        .collect::<Vec<_>>();
    inputs.resize(psbt.unsigned_tx.output.len(), "".to_string());
    let outputs = psbt
        .unsigned_tx
        .output
        .iter()
        .map(|e| {
            format!(
                "{}\n{}",
                Address::from_script(e.script_pubkey.as_script(), network).unwrap(),
                e.value
            )
        })
        .collect::<Vec<_>>();
    let rows = inputs
        .into_iter()
        .zip(outputs.into_iter())
        .map(|e| vec![e.0, e.1])
        .collect::<Vec<_>>();
    let mut table = comfy_table::Table::new();

    table.set_header(vec!["Inputs", "Outputs"]);

    for row in rows {
        table.add_row(row);
    }
    println!("{}", table);
}

pub(crate) fn select_confirm(msg: &str) -> bool {
    let selected = select(msg, &["Yes", "No"]);
    selected == 0
}
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use bitcoin::Address;

    #[test]
    fn test() {
        let a = vec![1, 2];
        let b = vec![2, 3, 4];
        let a = Address::from_str(
            "OP_HASH160 OP_PUSHBYTES_20 073f1424d780a7a96897d3d756788c6bdfaa12ac OP_EQUAL",
        )
        .unwrap();
    }
}
