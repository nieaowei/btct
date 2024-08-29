use std::{str::FromStr, thread, time::Duration};

use anyhow::{anyhow, bail};
use bdk::psbt::PsbtUtils;
use bitcoin::{
    absolute::LockTime, consensus::encode, psbt, psbt::PsbtSighashType, transaction::Version,
    ScriptBuf, TapSighashType,
};
use clap::ValueEnum;
use console_utils::input::select;
use log::log;
use miniscript::bitcoin::{
    psbt::Input, Address, Amount, FeeRate, OutPoint, Psbt, Sequence, Transaction, TxIn, TxOut,
};
use ordinals::{Edict, Runestone};

use crate::{
    btc_api::{
        esplora,
        esplora::{model::Utxo, Prevout},
        ordinal,
        ordinal::Ordinal,
        TxApi,
    },
    constant::{APPEND_NETWORK_FEE_SAT, DUMMY_UTXO, MIN_UTXO, POSTAGE},
    default,
    dummy_transaction::DummyTransaction,
    monitor,
    setting::Settings,
    utils, Print,
};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Type {
    Auto,
    NFT,
    Rune,
}

pub fn snipe(
    settings: Settings,
    tx_id: &str,
    addr: &str,
    typ: Type,
    increase_rate: u64,
    broadcast: bool,
    show_hex: bool,
    yes: bool,
    poison: bool,
    simple: bool,
    split_rate: Option<u64>,
    split_recv_addr: Option<String>,
    check: Option<String>,
    monitor: bool,
) -> anyhow::Result<()> {
    let btc_api = settings.btc_api();
    let ordi_api = settings.ordi_api();

    let wallet = settings.wallet()?;
    let pay_addr = wallet.pay_addr();
    let ordi_addr = wallet.ordi_addr();
    wallet.check();

    let mut tx_id = if !tx_id.is_empty() {
        tx_id.to_string()
    } else if !addr.is_empty() {
        let txs = btc_api.get_transactions(addr)?;

        txs.first()
            .ok_or(anyhow!("Not found addr txs"))?
            .txid
            .clone()
    } else {
        bail!("Error address")
    };

    // found rbf
    let rbf = btc_api.get_rbf_tx(&tx_id)?;
    if rbf.replacements.is_some() {
        if rbf.replacements.as_ref().unwrap().mined {
            bail!("[RBF] Tx confirmed, can not replaced");
        }
        log::info!(
            "[RBF] Origin tx has replaced: {}",
            rbf.replacements.as_ref().unwrap().tx.txid
        );
        tx_id = rbf.replacements.as_ref().unwrap().tx.txid.clone();
    }

    let snipe_pool_tx = btc_api.get_transaction(&tx_id)?;
    let snipe_tx = btc_api.get_btc_transaction(&tx_id)?;

    if rbf.replacements.is_some() {
        log::info!(
            "[RBF] New tx info: |FeeRate: {:.1} sat/vb||",
            snipe_pool_tx.fee as f64 / snipe_tx.vsize() as f64
        );
        if !yes {
            let selected_index = select("Try snipe new tx ?", &["No", "Yes"]);
            if selected_index == 0 {
                return Ok(());
            }
        }
    }

    let (can_utxos, dummy_utxos) = get_utxos(&btc_api, &pay_addr.to_string())?;

    let fee_rate =
        FeeRate::from_sat_per_vb(snipe_pool_tx.fee / snipe_tx.vsize() as u64 + increase_rate)
            .unwrap();

    let (mut unsigned_psbt, ordinals) = if simple {
        build_uncompleted_psbt_without_dummy(
            &settings,
            &btc_api,
            &ordi_api,
            &snipe_pool_tx,
            &snipe_tx,
            can_utxos,
            pay_addr.clone(),
            ordi_addr.clone(),
            typ,
            fee_rate,
        )?
    } else {
        build_uncompleted_psbt(
            &settings,
            &btc_api,
            &ordi_api,
            &snipe_pool_tx,
            &snipe_tx,
            can_utxos,
            dummy_utxos,
            pay_addr.clone(),
            ordi_addr.clone(),
            typ,
            fee_rate,
            poison,
        )?
    };

    if show_hex {
        log::info!("[unsigned PSBT] {}", unsigned_psbt.serialize_hex());
    }
    let ok = wallet.sign(&mut unsigned_psbt)?;
    if !ok {
        bail!("Sign failed")
    }

    let signed_tx = unsigned_psbt.clone().extract_tx()?;
    let hex = encode::serialize_hex(&signed_tx);

    if show_hex {
        log::info!("[signed] {}", hex);
    }
    let fee_rate = unsigned_psbt.fee_amount().unwrap_or(0) as f64 / signed_tx.vsize() as f64;

    log::info!(
        "[{}] FeeRate: {:.1} sat/vb , TotalFee: {} sat , Size: {} vb",
        signed_tx.txid(),
        fee_rate,
        unsigned_psbt.fee_amount().unwrap_or(0),
        signed_tx.vsize()
    );
    utils::print_snipe_table(&unsigned_psbt, ordinals, settings.network);

    let split_psbt = if simple {
        let outpoint = OutPoint {
            txid: signed_tx.txid(),
            vout: 0,
        };
        let amount = signed_tx.output.first().unwrap().value;
        let split_rate = split_rate
            .map(|e| FeeRate::from_sat_per_vb(e).unwrap())
            .unwrap_or(unsigned_psbt.fee_rate().unwrap());

        let mut psbt = build_split_rune_psbt(
            &settings,
            (outpoint, amount),
            ordi_addr,
            split_recv_addr
                .clone()
                .map_or(Ok(pay_addr.as_unchecked().clone()), |e| {
                    Address::from_str(&e)
                })?
                .assume_checked(),
            pay_addr.clone(),
            split_rate,
        )?;
        if show_hex {
            log::info!("[unsigned psbt] {} ", psbt.serialize_hex())
        }

        let ok = wallet.sign_swap(&mut psbt)?;
        if !ok {
            bail!("Split rune sign failed")
        }
        let signed_tx = psbt.clone().extract_tx()?;

        log::info!(
            "[{}] FeeRate: {:.1} sat/vb , TotalFee: {} sat , Size: {} vb",
            signed_tx.txid(),
            split_rate.to_sat_per_vb_ceil(),
            psbt.fee_amount().unwrap_or(0),
            signed_tx.vsize()
        );
        utils::print_table(&psbt, settings.network);
        Some(encode::serialize_hex(&signed_tx))
    } else {
        None
    };
    if broadcast {
        let selected = select("Please confirm cost: ", &["No", "Yes"]);
        if selected == 0 {
            log::info!("You have canceled the sniper, and the transaction did not take effect.");
            return Ok(());
        }

        settings.broadcast(&hex)?;
        if let Some(split_rune_hex) = &split_psbt {
            settings.broadcast(split_rune_hex)?;
        };
        if let Some(rpc_api) = settings.rpc_api() {
            rpc_api.send_raw_tx(&hex);
            if let Some(split_rune_hex) = split_psbt {
                rpc_api.send_raw_tx(&split_rune_hex);
            };
        }

        // monitor
        if monitor {
            if let ok = monitor::monitor(&settings, &signed_tx.txid().to_string(), 3)? {
                if !ok {
                    return snipe(
                        settings,
                        &tx_id,
                        addr,
                        typ,
                        increase_rate,
                        broadcast,
                        show_hex,
                        yes,
                        poison,
                        simple,
                        split_rate,
                        split_recv_addr,
                        check,
                        monitor,
                    );
                }
            }
        }
    }

    Ok(())
}

/// (available_utxo , dummy_utxo)
pub(crate) fn get_utxos(
    btc_api: &esplora::Client,
    addr: &str,
) -> anyhow::Result<(Vec<Utxo>, Vec<Utxo>)> {
    // utxos dummy utxo
    let mut utxos = btc_api.get_utxo(addr)?;
    utxos.sort_by(|a, b| b.value.cmp(&a.value));

    let utxos = utxos
        .into_iter()
        .filter(|e| e.status.confirmed)
        .filter(|e| e.value > Amount::from_sat(546))
        .collect::<Vec<_>>();
    // 过滤掉铭文

    let (dummy_utxos, utxos) = utxos
        .into_iter()
        .filter(|e| {
            // 筛选 dummy utxo 和 大于 10000sat的 utxo （防止误操作铭文资产） 后续可改api直接查询 okx wass api
            if e.value == DUMMY_UTXO {
                return true;
            }
            if e.value > MIN_UTXO {
                return true;
            }
            return false;
        })
        .partition(|e| e.value == DUMMY_UTXO);

    Ok((utxos, dummy_utxos))
}

/// get utxo of fixed value
pub(crate) fn get_value_utxos(
    btc_api: &esplora::Client,
    addr: &str,
    value: Amount,
) -> anyhow::Result<Vec<Utxo>> {
    // utxos dummy utxo
    let mut utxos = btc_api.get_utxo(addr)?;
    utxos.sort_by(|a, b| b.value.cmp(&a.value));

    let utxos = utxos
        .into_iter()
        .filter(|e| e.status.confirmed)
        .filter(|e| e.value == value)
        .collect::<Vec<_>>();
    // 过滤掉铭文

    Ok(utxos)
}

fn build_split_rune_psbt(
    settings: &Settings,
    (outpoint, amount): (OutPoint, Amount),
    pay_addr: Address,
    recv_addr: Address,
    change_addr: Address,
    fee_rate: FeeRate,
) -> anyhow::Result<Psbt> {
    let mut unsigned_tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: vec![],
    };
    let mut dummy_tx = DummyTransaction::new();
    let mut psbt_inputs = Vec::new();
    // input rune
    unsigned_tx.input.push(TxIn {
        previous_output: outpoint,
        script_sig: Default::default(),
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        witness: Default::default(),
    });

    psbt_inputs.push(psbt::Input {
        witness_utxo: Some(TxOut {
            value: amount,
            script_pubkey: pay_addr.script_pubkey(),
        }),
        ..default()
    });
    dummy_tx.append_input(pay_addr, None, None);

    // rune index
    unsigned_tx.output.push(TxOut {
        value: POSTAGE,
        script_pubkey: recv_addr.script_pubkey(),
    });
    dummy_tx.append_output(recv_addr.script_pubkey());

    // change
    unsigned_tx.output.push(TxOut {
        value: Amount::ZERO,
        script_pubkey: change_addr.script_pubkey(),
    });
    dummy_tx.append_output(change_addr.script_pubkey());

    let network_fee = fee_rate.fee_vb(dummy_tx.vsize() as u64).unwrap();
    unsigned_tx.output.last_mut().unwrap().value = amount - network_fee - POSTAGE;

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

fn build_uncompleted_psbt(
    settings: &Settings,
    btc_api: &esplora::Client,
    ordi_api: &ordinal::Client,
    snipe_pool_tx: &esplora::Transaction,
    snipe_tx: &Transaction,
    cardinal_utxos: Vec<Utxo>,
    mut dummy_utxos: Vec<Utxo>,
    pay_addr: Address,
    rev_addr: Address,
    typ: Type,
    fee_rate: FeeRate,
    poison: bool,
) -> anyhow::Result<(Psbt, Vec<(usize, Ordinal, (TxIn, Prevout), TxOut)>)> {
    if snipe_pool_tx.status.confirmed {
        bail!("Origin tx confirmed, can not replaced");
    }

    let mut dummy_signed_tx_1 = DummyTransaction::new();

    log::info!(
        "[origin tx] TotalFee: {} sat, FeeRate: {:.1} sat/vb, Size: {} vb ",
        snipe_pool_tx.fee,
        (snipe_pool_tx.fee as f64 / snipe_tx.vsize() as f64),
        snipe_tx.vsize(),
    );

    let mut inputs = Vec::new();
    let mut signed_psbt_inputs = Vec::new();
    let mut inputs_amount = Amount::ZERO;

    let mut outputs = Vec::new();
    let mut outputs_amount = Amount::ZERO;

    let mut is_rune = typ == Type::Rune;
    let snipe_utxos = snipe_tx
        .input
        .iter()
        .map(|e| &e.previous_output)
        .collect::<Vec<_>>();

    log::info!("[waiting] Founding inscription and rune from origin tx");
    let ordinals = ordi_api.fetch_outputs(snipe_utxos)?;

    let mut ordinal_and_output = ordinals
        .into_iter()
        .map(|(i, ordi)| {
            (
                i,
                ordi,
                (
                    snipe_tx.input[i].clone(),
                    snipe_pool_tx.vin[i].prevout.clone(),
                ),
                snipe_tx.output[i].clone(),
            )
        })
        .collect::<Vec<_>>();

    if ordinal_and_output.len() < 1 {
        bail!("Not found inscription or rune");
    }

    let mut selected = if ordinal_and_output.len() > 1 {
        let ordinal_options = ordinal_and_output
            .iter()
            .map(|(_, ordinal, _, txout)| ordinal.display_value(txout.value))
            .collect::<Vec<_>>();
        let selected = console_utils::input::multiselect(
            "Multiple runes or inscriptions were found. Please select one or more (By SPACE Key)",
            &ordinal_options
                .iter()
                .map(|e| e.as_str())
                .collect::<Vec<_>>(),
        );
        selected
    } else {
        vec![true; ordinal_and_output.len()]
    };

    if poison {
        // found first selected
        let amount = ordinal_and_output
            .iter()
            .enumerate()
            .find(|(i, _)| selected[*i])
            .map(|e| e.1 .3.value)
            .unwrap();
        let (ordinal, (txin, prevout), txout) =
            get_poison(settings, btc_api, ordi_api, amount, typ)?;
        let mut selected_copy = vec![true];
        selected_copy.append(&mut selected);
        selected = selected_copy;

        let mut ordinal_and_output_copy = vec![(0, ordinal, (txin, prevout), txout)];
        ordinal_and_output_copy.append(&mut ordinal_and_output);
        ordinal_and_output = ordinal_and_output_copy;
    }

    let mut edicts: Vec<Edict> = Vec::new();
    let mut edict_index = 0;

    for (index, (_, ordinal, (txin, prevout), txout)) in ordinal_and_output.iter().enumerate() {
        if !selected[index] {
            continue;
        }
        let (_out_point, _value) = match ordinal {
            Ordinal::None => {
                bail!("Not found inscription or rune");
            }
            Ordinal::Inscription {
                id,
                value,
                out_point,
            } => {
                log::info!(
                    "[found inscription] Id: https://ordinals.com/{} , Value: {} , Output: {}",
                    id,
                    value.to_sat(),
                    out_point
                );
                (out_point, value)
            }
            Ordinal::Rune {
                id,
                name,
                value,
                number,
                div,
                out_point,
            } => {
                log::info!(
                    "[found rune] Id: {}, Name: {} , Value: {} , Number: {} Output: {}, Div: {}",
                    id,
                    name,
                    value.to_sat(),
                    number,
                    out_point,
                    div
                );
                edicts.push(Edict {
                    id: id.clone(),
                    amount: div
                        .eq(&0)
                        .then_some(*number)
                        .unwrap_or(number * 10u128.pow(*div)),
                    output: edict_index,
                });
                is_rune = true;
                (out_point, value)
            }
        };

        inputs.push({
            TxIn {
                previous_output: txin.previous_output.clone(),
                sequence: txin.sequence,
                ..Default::default()
            }
        });
        dummy_signed_tx_1.append_input(
            Address::from_str(&prevout.scriptpubkey_address)?.assume_checked(),
            Some(txin.script_sig.clone()),
            Some(txin.witness.clone()),
        );

        signed_psbt_inputs.push(Input {
            witness_utxo: Some({
                TxOut {
                    value: prevout.value,
                    script_pubkey: ScriptBuf::from_hex(&prevout.scriptpubkey)?,
                }
            }),
            final_script_witness: Some(txin.witness.clone()), // 可以优化少打一个请求 直接解析pool
            final_script_sig: Some(txin.script_sig.clone()),
            ..Default::default()
        });

        outputs.push(txout.clone());
        dummy_signed_tx_1.append_output(txout.script_pubkey.clone());
        outputs_amount += txout.value;
        edict_index += 1;
    }
    // if inputs.len() < 1 {
    //     bail!("No selected inscription or rune")
    // }

    let mut buyer_unsigned_tx = Transaction {
        version: snipe_tx.version,
        lock_time: snipe_tx.lock_time,
        input: inputs,
        output: outputs,
    };

    // build self

    let dummy_utxo = dummy_utxos.pop().ok_or(anyhow!("No dummy utxo"))?;
    let (mut inputs, mut psbt_inputs) = {
        if !is_rune {
            dummy_signed_tx_1.append_input(pay_addr.clone(), None, None);
            inputs_amount += dummy_utxo.value;

            (
                vec![{
                    TxIn {
                        // 初始化
                        previous_output: OutPoint {
                            txid: dummy_utxo.txid,
                            vout: dummy_utxo.vout,
                        },
                        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                        ..Default::default()
                    }
                }],
                vec![{
                    Input {
                        witness_utxo: Some({
                            TxOut {
                                value: dummy_utxo.value,
                                script_pubkey: pay_addr.script_pubkey(),
                            }
                        }),
                        ..Default::default()
                    }
                }],
            )
        } else {
            // 符文不需要 归集
            // inputs_amount += dummy_utxo.value;
            (vec![], vec![])
        }
    };

    // if is_rune {
    //     inputs_amount += dummy_utxo.value;
    // }

    // 初始化 dummy 归集
    let mut outputs = if !is_rune {
        dummy_signed_tx_1.append_output(pay_addr.script_pubkey());

        outputs_amount += dummy_utxo.value;

        vec![{
            TxOut {
                value: dummy_utxo.value,
                script_pubkey: pay_addr.script_pubkey(),
            }
        }]
    } else {
        vec![]
    };
    // 资产占位
    for _ in 0..buyer_unsigned_tx.input.len() {
        let dummy_utxo = dummy_utxos.pop().ok_or(anyhow!("No dummy utxo"))?;

        inputs.push({
            TxIn {
                previous_output: OutPoint {
                    txid: dummy_utxo.txid,
                    vout: dummy_utxo.vout,
                },
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                ..Default::default()
            }
        });

        dummy_signed_tx_1.append_input(pay_addr.clone(), None, None);

        psbt_inputs.push({
            Input {
                witness_utxo: Some({
                    TxOut {
                        value: dummy_utxo.value,
                        script_pubkey: pay_addr.script_pubkey(),
                    }
                }),
                // non_witness_utxo: non_witness_utxo,
                ..Default::default()
            }
        });
        inputs_amount += dummy_utxo.value;

        if !is_rune {
            outputs.first_mut().unwrap().value += dummy_utxo.value;
        }

        outputs.push({
            TxOut {
                value: POSTAGE,
                script_pubkey: rev_addr.script_pubkey(),
            }
        });

        dummy_signed_tx_1.append_output(rev_addr.script_pubkey());

        outputs_amount += POSTAGE;
    }

    // merge output
    outputs.append(&mut buyer_unsigned_tx.output);
    // gen dummy utxo
    if !is_rune {
        for _i in 0..inputs.len() {
            outputs.push({
                TxOut {
                    value: DUMMY_UTXO,
                    script_pubkey: pay_addr.script_pubkey(),
                }
            });

            dummy_signed_tx_1.append_output(pay_addr.script_pubkey());

            outputs_amount += DUMMY_UTXO;
        }
    } else {
        outputs.push({
            TxOut {
                value: DUMMY_UTXO,
                script_pubkey: pay_addr.script_pubkey(),
            }
        });

        dummy_signed_tx_1.append_output(pay_addr.script_pubkey());

        outputs_amount += DUMMY_UTXO;
    }
    // merge input
    inputs.append(&mut buyer_unsigned_tx.input);
    psbt_inputs.append(&mut signed_psbt_inputs);

    // rune output
    if is_rune && edict_index > 1 {
        let sp = Runestone {
            edicts,
            etching: None,
            mint: None,
            pointer: None,
        }
        .encipher();
        outputs.push(TxOut {
            value: Amount::ZERO,
            script_pubkey: ScriptBuf::from_bytes(sp.clone().into_bytes()),
        });
        dummy_signed_tx_1.append_output(ScriptBuf::from_bytes(sp.into_bytes()));
    }

    // change output
    outputs.push({
        TxOut {
            value: Amount::ZERO,
            script_pubkey: pay_addr.script_pubkey(),
        }
    });

    dummy_signed_tx_1.append_output(pay_addr.script_pubkey());

    let mut unsigned_tx = Transaction {
        version: buyer_unsigned_tx.version,
        lock_time: buyer_unsigned_tx.lock_time,
        input: inputs,
        output: outputs,
    };

    let need_amount = outputs_amount - inputs_amount; // 需要的
    let mut extra_network_fee = Amount::ZERO; // RBF需要总交易费用大于原始交易
    let mut amount = Amount::ZERO; // 计算
    let mut ok = false;

    'outer: for utxo in cardinal_utxos {
        amount += utxo.value;

        psbt_inputs.push({
            Input {
                witness_utxo: Some({
                    TxOut {
                        value: utxo.value,
                        script_pubkey: pay_addr.script_pubkey(),
                    }
                }),
                ..Default::default()
            }
        });

        unsigned_tx.input.push({
            TxIn {
                previous_output: OutPoint {
                    txid: utxo.txid,
                    vout: utxo.vout,
                },
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                ..Default::default()
            }
        });

        dummy_signed_tx_1.append_input(pay_addr.clone(), None, None);

        // let a = dummy_signed_tx.base_size();
        // let b = dummy_signed_tx.vsize();
        // let c = dummy_signed_tx.total_size();

        let network_fee = fee_rate.fee_vb(dummy_signed_tx_1.vsize() as u64).unwrap();

        log::info!("[estimate] Size: {} vb", dummy_signed_tx_1.vsize());

        loop {
            if let Some(unfilled) =
                amount.checked_sub(network_fee + need_amount + extra_network_fee)
            {
                if (network_fee + extra_network_fee) > Amount::from_sat(snipe_pool_tx.fee + 1000) {
                    // 大于原始交易的总费用才能上链
                    unsigned_tx.output.last_mut().unwrap().value = unfilled; // 找零
                    ok = true;
                    break 'outer;
                }
                // 不够就追加
                extra_network_fee += APPEND_NETWORK_FEE_SAT;
                log::info!(
                    "[network fee append] {} sat",
                    APPEND_NETWORK_FEE_SAT.to_sat()
                );
            } else {
                continue 'outer;
            }
        }
    }
    if !ok {
        bail!("No utxo or utxo not enough")
    }

    // 找零小于粉尘值
    if unsigned_tx.output.last().unwrap().value < pay_addr.script_pubkey().dust_value() {
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
        outputs: vec![Default::default(); o_len],
    };

    let cost = ordinal_and_output
        .iter()
        .enumerate()
        .filter(|(index, _)| selected[*index])
        .map(|(_, (_, _, _, txout))| txout.value)
        .sum::<Amount>()
        + (psbt
            .inputs
            .iter()
            .map(|e| e.witness_utxo.as_ref().unwrap().value)
            .sum::<Amount>()
            - psbt
                .unsigned_tx
                .output
                .iter()
                .map(|e| e.value)
                .sum::<Amount>()); // amount - psbt.unsigned_tx.output.last().unwrap().value
    let arg = if is_rune {
        cost.to_sat() as f64
            / ordinal_and_output
                .iter()
                .enumerate()
                .filter(|(index, _)| selected[*index])
                .filter(|(_, (_, o, _, _))| o.is_rune())
                .map(|(_, (_, o, _, txout))| {
                    if let Ordinal::Rune { number, .. } = o {
                        *number
                    } else {
                        0
                    }
                })
                .sum::<u128>() as f64
    } else {
        0f64
    };
    log::info!("[cost] Total: {} ,Average: {}", cost, arg);
    Ok((psbt, ordinal_and_output))
}

fn get_poison(
    settings: &Settings,
    btc_api: &esplora::Client,
    ordi_api: &ordinal::Client,
    pay_amount: Amount,
    typ: Type,
) -> anyhow::Result<(Ordinal, (TxIn, Prevout), TxOut)> {
    let wallet = settings.poison_wallet()?;
    log::info!("[PoisonWallet] Pay: {} ", wallet.pay_addr());
    log::info!("[PoisonWallet] Ordi: {} ", wallet.ordi_addr());

    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: vec![],
    };
    // 挑选一组符文
    let utxos = get_value_utxos(btc_api, &wallet.ordi_addr().to_string(), POSTAGE)?;
    let ordi_utxos = utxos
        .iter()
        .map(|e| OutPoint {
            txid: e.txid,
            vout: e.vout,
        })
        .collect::<Vec<_>>();
    let ordi = if typ == Type::NFT {
        ordi_api.fetch_one_inscription_output(ordi_utxos.iter().collect())?
    } else {
        ordi_api.fetch_one_rune_output(ordi_utxos.iter().collect())?
    };

    let outpoint = ordi.outpoint().ok_or(anyhow!("No posion ordinal"))?;
    log::info!("[poison] {} ", ordi.display());
    tx.input.push(TxIn {
        previous_output: outpoint.clone(),
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        ..default()
    });
    tx.output.push(TxOut {
        value: pay_amount,
        script_pubkey: wallet.pay_addr().script_pubkey(),
    });

    let mut psbt = Psbt {
        unsigned_tx: tx,
        version: 0,
        xpub: Default::default(),
        proprietary: Default::default(),
        unknown: Default::default(),
        inputs: vec![],
        outputs: vec![default(); 1],
    };

    psbt.inputs.push(Input {
        witness_utxo: Some({
            TxOut {
                value: POSTAGE,
                script_pubkey: wallet.ordi_addr().script_pubkey(),
            }
        }),
        sighash_type: Some(PsbtSighashType::from(
            TapSighashType::SinglePlusAnyoneCanPay,
        )),
        ..default()
    });

    let ok = wallet.sign_swap(&mut psbt)?;
    if !ok {
        bail!("Sign failed")
    }

    psbt.serialize_hex().print();

    let input = psbt.unsigned_tx.input.pop().unwrap();
    let psbt_input = psbt.inputs.pop().unwrap();
    Ok((
        ordi,
        (
            TxIn {
                previous_output: input.previous_output,
                script_sig: psbt_input.final_script_sig.unwrap(),
                sequence: input.sequence,
                witness: psbt_input.final_script_witness.unwrap(),
            },
            Prevout {
                scriptpubkey: wallet.ordi_addr().script_pubkey().to_hex_string(),
                scriptpubkey_asm: "".to_string(),
                scriptpubkey_type: "".to_string(),
                scriptpubkey_address: wallet.ordi_addr().to_string(),
                value: POSTAGE,
            },
        ),
        psbt.unsigned_tx.output.pop().unwrap(),
    ))
}

fn build_uncompleted_psbt_without_dummy(
    settings: &Settings,
    btc_api: &esplora::Client,
    ordi_api: &ordinal::Client,
    snipe_pool_tx: &esplora::Transaction,
    snipe_tx: &Transaction,
    cardinal_utxos: Vec<Utxo>,
    pay_addr: Address,
    rev_addr: Address,
    typ: Type,
    fee_rate: FeeRate,
) -> anyhow::Result<(Psbt, Vec<(usize, Ordinal, (TxIn, Prevout), TxOut)>)> {
    if snipe_pool_tx.status.confirmed {
        bail!("Origin tx confirmed, can not replaced");
    }

    let mut dummy_signed_tx_1 = DummyTransaction::new();

    log::info!(
        "[origin tx] TotalFee: {} sat, FeeRate: {:.1} sat/vb, Size: {} vb ",
        snipe_pool_tx.fee,
        (snipe_pool_tx.fee as f64 / snipe_tx.vsize() as f64),
        snipe_tx.vsize(),
    );

    let mut inputs = Vec::new();
    let mut signed_psbt_inputs = Vec::new();
    let mut inputs_amount = Amount::ZERO;

    let mut outputs = Vec::new();
    let mut outputs_amount = Amount::ZERO;

    let mut is_rune = typ == Type::Rune;
    let snipe_utxos = snipe_tx
        .input
        .iter()
        .map(|e| &e.previous_output)
        .collect::<Vec<_>>();

    log::info!("[waiting] Founding inscription and rune from origin tx");
    let ordinals = ordi_api.fetch_outputs(snipe_utxos)?;

    let mut ordinal_and_output = ordinals
        .into_iter()
        .map(|(i, ordi)| {
            (
                i,
                ordi,
                (
                    snipe_tx.input[i].clone(),
                    snipe_pool_tx.vin[i].prevout.clone(),
                ),
                snipe_tx.output[i].clone(),
            )
        })
        .collect::<Vec<_>>();

    if ordinal_and_output.len() < 1 {
        bail!("Not found inscription or rune");
    }

    let mut selected = if ordinal_and_output.len() > 1 {
        let ordinal_options = ordinal_and_output
            .iter()
            .map(|(_, ordinal, _, txout)| ordinal.display_value(txout.value))
            .collect::<Vec<_>>();
        let selected = console_utils::input::multiselect(
            "Multiple runes or inscriptions were found. Please select one or more (By SPACE Key)",
            &ordinal_options
                .iter()
                .map(|e| e.as_str())
                .collect::<Vec<_>>(),
        );
        selected
    } else {
        vec![true; ordinal_and_output.len()]
    };

    for (index, (_, ordinal, (txin, prevout), txout)) in ordinal_and_output.iter().enumerate() {
        if !selected[index] {
            continue;
        }
        let (_out_point, _value) = match ordinal {
            Ordinal::None => {
                bail!("Not found inscription or rune");
            }
            Ordinal::Inscription {
                id,
                value,
                out_point,
            } => {
                log::info!(
                    "[found inscription] Id: https://ordinals.com/{} , Value: {} , Output: {}",
                    id,
                    value.to_sat(),
                    out_point
                );
                (out_point, value)
            }
            Ordinal::Rune {
                id,
                name,
                value,
                number,
                div,
                out_point,
            } => {
                log::info!(
                    "[found rune] Id: {}, Name: {} , Value: {} , Number: {} Output: {}, Div: {}",
                    id,
                    name,
                    value.to_sat(),
                    number,
                    out_point,
                    div
                );
                is_rune = true;
                (out_point, value)
            }
        };

        inputs.push({
            TxIn {
                previous_output: txin.previous_output.clone(),
                sequence: txin.sequence,
                ..Default::default()
            }
        });
        dummy_signed_tx_1.append_input(
            Address::from_str(&prevout.scriptpubkey_address)?.assume_checked(),
            Some(txin.script_sig.clone()),
            Some(txin.witness.clone()),
        );

        signed_psbt_inputs.push(Input {
            witness_utxo: Some({
                TxOut {
                    value: prevout.value,
                    script_pubkey: ScriptBuf::from_hex(&prevout.scriptpubkey)?,
                }
            }),
            final_script_witness: Some(txin.witness.clone()), // 可以优化少打一个请求 直接解析pool
            final_script_sig: Some(txin.script_sig.clone()),
            ..Default::default()
        });

        outputs.push(txout.clone());
        dummy_signed_tx_1.append_output(txout.script_pubkey.clone());
        outputs_amount += txout.value;
    }
    // if inputs.len() < 1 {
    //     bail!("No selected inscription or rune")
    // }

    let mut buyer_unsigned_tx = Transaction {
        version: snipe_tx.version,
        lock_time: snipe_tx.lock_time,
        input: inputs,
        output: outputs,
    };

    // 占位和找零共用
    buyer_unsigned_tx.output.insert(
        0,
        TxOut {
            value: Amount::ZERO,
            script_pubkey: rev_addr.script_pubkey(),
        },
    );
    dummy_signed_tx_1.append_output(rev_addr.script_pubkey());

    let mut unsigned_tx = buyer_unsigned_tx;
    // merge output
    let mut psbt_inputs = signed_psbt_inputs;
    let need_amount = outputs_amount - inputs_amount; // 需要的
    let mut extra_network_fee = Amount::ZERO; // RBF需要总交易费用大于原始交易
    let mut amount = Amount::ZERO; // 计算
    let mut ok = false;
    let mut init = false; // 第一个input填充

    'outer: for utxo in cardinal_utxos {
        amount += utxo.value;

        if !init {
            psbt_inputs.insert(0, {
                Input {
                    witness_utxo: Some({
                        TxOut {
                            value: utxo.value,
                            script_pubkey: pay_addr.script_pubkey(),
                        }
                    }),
                    ..Default::default()
                }
            });

            unsigned_tx.input.insert(0, {
                TxIn {
                    previous_output: OutPoint {
                        txid: utxo.txid,
                        vout: utxo.vout,
                    },
                    sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                    ..Default::default()
                }
            });
            init = true;
        } else {
            psbt_inputs.push({
                Input {
                    witness_utxo: Some({
                        TxOut {
                            value: utxo.value,
                            script_pubkey: pay_addr.script_pubkey(),
                        }
                    }),
                    ..Default::default()
                }
            });

            unsigned_tx.input.push({
                TxIn {
                    previous_output: OutPoint {
                        txid: utxo.txid,
                        vout: utxo.vout,
                    },
                    sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                    ..Default::default()
                }
            });
        }
        dummy_signed_tx_1.append_input(pay_addr.clone(), None, None);

        let network_fee = fee_rate.fee_vb(dummy_signed_tx_1.vsize() as u64).unwrap();

        log::info!("[estimate] Size: {} vb", dummy_signed_tx_1.vsize());

        loop {
            if let Some(unfilled) =
                amount.checked_sub(network_fee + need_amount + extra_network_fee)
            {
                // 找零小于粉尘值
                if unfilled < pay_addr.script_pubkey().dust_value() {
                    break;
                }

                if (network_fee + extra_network_fee) > Amount::from_sat(snipe_pool_tx.fee + 1000) {
                    // 大于原始交易的总费用才能上链
                    unsigned_tx.output.first_mut().unwrap().value = unfilled; // 找零

                    ok = true;
                    break 'outer;
                }
                // 不够就追加
                extra_network_fee += APPEND_NETWORK_FEE_SAT;
                log::info!(
                    "[network fee append] {} sat",
                    APPEND_NETWORK_FEE_SAT.to_sat()
                );
            } else {
                continue 'outer;
            }
        }
    }
    if !ok {
        bail!("No utxo or utxo not enough")
    }

    let o_len = unsigned_tx.output.len();
    let psbt = Psbt {
        unsigned_tx,
        version: 0,
        xpub: Default::default(),
        proprietary: Default::default(),
        unknown: Default::default(),
        inputs: psbt_inputs,
        outputs: vec![Default::default(); o_len],
    };
    let cost = ordinal_and_output
        .iter()
        .enumerate()
        .filter(|(index, _)| selected[*index])
        .map(|(_, (_, _, _, txout))| txout.value)
        .sum::<Amount>()
        + (psbt
            .inputs
            .iter()
            .map(|e| e.witness_utxo.as_ref().unwrap().value)
            .sum::<Amount>()
            - psbt
                .unsigned_tx
                .output
                .iter()
                .map(|e| e.value)
                .sum::<Amount>()); // amount - psbt.unsigned_tx.output.last().unwrap().value
    let arg = if is_rune {
        cost.to_sat() as f64
            / ordinal_and_output
                .iter()
                .enumerate()
                .filter(|(index, _)| selected[*index])
                .filter(|(_, (_, o, _, _))| o.is_rune())
                .map(|(_, (_, o, _, txout))| {
                    if let Ordinal::Rune { number, .. } = o {
                        *number
                    } else {
                        0
                    }
                })
                .sum::<u128>() as f64
    } else {
        0f64
    };
    log::info!("[cost] Total: {} ,Average: {}", cost, arg);
    Ok((psbt, ordinal_and_output))
}

// pub fn _snipe(settings: Settings, tx_id: &str, rate_fee: u64) -> anyhow::Result<()> {
//     let btc_api = settings.btc_api();
//     let mut wallet = settings.wallet()?;
//     let secp = &wallet.secp;
//
//     let addr = wallet.account0_addr()?;
//
//     let sp = addr.script_pubkey();
//     // let descriptor = Descriptor::new_tr(wallet.account0_xpub()?.public_key.to_x_only_pubkey(), None)?;
//     addr.print();
//
//     let fee_rate = FeeRate::from_sat_per_vb(rate_fee).unwrap();
//     let origin_tx = btc_api.get_btc_transaction(tx_id)?;
//     let pool_tx = btc_api.get_transaction(tx_id)?;
//
//     let mut buyer_addr = None;
//
//     let mut new_inputs = Vec::new();
//     let mut new_inputs_completed = Vec::new();
//     let mut new_outputs = Vec::new();
//
//     for (i, input) in pool_tx.vin.iter().enumerate() {
//         if input.prevout.value <= Amount::from_sat(546) {
//             new_inputs.push(TxIn {
//                 previous_output: origin_tx.input[i].previous_output,
//                 sequence: origin_tx.input[i].sequence,
//                 ..Default::default()
//             });
//             new_inputs_completed.push(origin_tx.input[i].clone());
//         } else {
//             buyer_addr = Some(input.prevout.scriptpubkey_address.clone());
//         }
//     }
//     let Some(buyer_addr) = buyer_addr else {
//         bail!("No seller")
//     };
//
//     for (i, out) in pool_tx.vout.iter().enumerate() {
//         if (out.scriptpubkey_address != buyer_addr
//             && !settings.contain_me_fee(&out.scriptpubkey_address))
//             || out.scriptpubkey_address == ""
//         {
//             if out.value > Amount::from_sat(546) {
//                 new_outputs.push(origin_tx.output[i].clone());
//             }
//         }
//     }
//
//     // utxo
//     let utxos = btc_api.get_utxo(&addr.to_string())?;
//
//     let mut cardinal_utxos = utxos
//         .iter()
//         .filter(|utxo| utxo.value > Amount::from_sat(546))
//         .collect::<Vec<_>>();
//
//     cardinal_utxos.sort_by(|a, b| b.value.cmp(&a.value));
//
//     let mut transaction = Transaction {
//         version: Version::TWO,
//         lock_time: LockTime::ZERO,
//         input: new_inputs, // 可以添加多个 input
//         output: vec![],    // 可以添加多个 output
//     };
//
//     let mut transfer_amount = Amount::from_sat(0); // 需要的
//
//     let mut psbt_outputs = Vec::new();
//
//     // 占位
//     for (i, _) in transaction.input.iter().enumerate() {
//         transaction.output.push(TxOut {
//             value: Amount::from_sat(546),
//             script_pubkey: sp.clone(),
//         });
//         psbt_outputs.push(psbt::Output {
//             witness_script: Some(transaction.input[i].script_sig.clone()),
//             ..Default::default()
//         });
//     }
//
//     // 初始化找零
//     transaction.output.push(TxOut {
//         value: Amount::ZERO,
//         script_pubkey: sp.clone(),
//     });
//     psbt_outputs.push(Default::default());
//     let change_index = transaction.output.len() - 1;
//
//     for new_output in new_outputs {
//         transfer_amount += new_output.value;
//         transaction.output.push(new_output);
//         psbt_outputs.push(Default::default());
//     }
//
//     for _ in 0..transaction.input.len() {
//         transfer_amount -= Amount::from_sat(546);
//     }
//
//     let mut psbt_inputs = vec![];
//     let mut input_txouts = Vec::<TxOut>::new();
//
//     let mut amount = Amount::from_sat(0);
//
//     // 别人的 utxo
//     for input in &transaction.input {
//         let non_witness_utxo =
//             btc_api.get_btc_transaction(&input.previous_output.txid.to_string())?;
//
//         input_txouts.push(TxOut {
//             value: non_witness_utxo.output[input.previous_output.vout as usize].value,
//             script_pubkey: non_witness_utxo.output[input.previous_output.vout as usize]
//                 .script_pubkey
//                 .clone(),
//         });
//
//         let psbt_input = Input {
//             witness_utxo: Some({
//                 TxOut {
//                     value: non_witness_utxo.output[input.previous_output.vout as usize].value,
//                     script_pubkey: non_witness_utxo.output[input.previous_output.vout as usize]
//                         .script_pubkey
//                         .clone(),
//                 }
//             }),
//             final_script_witness: Some(input.witness.clone()),
//             non_witness_utxo: Some(non_witness_utxo),
//             ..Default::default()
//         };
//
//         // psbt_input.update_with_descriptor_unchecked(&descriptor)?;
//
//         psbt_inputs.push(psbt_input);
//     }
//
//     let mut ok = false;
//
//     for utxo in cardinal_utxos {
//         amount += utxo.value;
//         let non_witness_utxo = btc_api.get_btc_transaction(&utxo.txid.to_string())?;
//
//         transaction.input.push(TxIn {
//             previous_output: OutPoint {
//                 txid: utxo.txid,
//                 vout: utxo.vout,
//             },
//             script_sig: ScriptBuf::new(),
//             sequence: Sequence::MAX,
//             witness: Witness::new(),
//         });
//
//         let mut origins = BTreeMap::new();
//
//         let input_pubkey = wallet.account0_xpub()?.public_key.to_x_only_pubkey();
//
//         origins.insert(
//             input_pubkey,
//             (
//                 vec![],
//                 (
//                     wallet.account0_xpriv()?.fingerprint(&secp),
//                     wallet.derivation_path.clone(),
//                 ),
//             ),
//         );
//
//         input_txouts.push(TxOut {
//             value: utxo.value,
//             script_pubkey: non_witness_utxo.output[utxo.vout as usize]
//                 .script_pubkey
//                 .clone(),
//         });
//
//         let psbt_input = Input {
//             witness_utxo: Some({
//                 TxOut {
//                     value: utxo.value,
//                     script_pubkey: non_witness_utxo.output[utxo.vout as usize]
//                         .script_pubkey
//                         .clone(),
//                 }
//             }),
//             sighash_type: Some(PsbtSighashType::from(TapSighashType::All)),
//             non_witness_utxo: Some(non_witness_utxo),
//             tap_key_origins: origins,
//             tap_internal_key: Some(input_pubkey),
//             ..Default::default()
//         };
//
//         psbt_inputs.push(psbt_input);
//
//         // fee cal
//         let network_fee = fee_rate.fee_vb(transaction.vsize() as u64).unwrap();
//         if let Some(unfilled) = amount.checked_sub(network_fee + transfer_amount) {
//             transaction.output[change_index].value = unfilled; // 找零
//
//             ok = true;
//             break;
//         }
//     }
//     if !ok {
//         return Err(anyhow::anyhow!("No utxo or utxo not enough"));
//     }
//
//     let mut psbt = Psbt {
//         unsigned_tx: transaction,
//         version: 0,
//         xpub: Default::default(),
//         proprietary: Default::default(),
//         unknown: Default::default(),
//         inputs: psbt_inputs,
//         outputs: psbt_outputs,
//     };
//
//     let unsigned_tx = psbt.unsigned_tx.clone();
//
//     psbt.inputs
//         .iter_mut()
//         .enumerate()
//         .try_for_each::<_, anyhow::Result<()>>(|(vout, input)| {
//             // no need sign
//
//             if input.final_script_witness.is_some() {
//                 return Ok(());
//             }
//
//             let sighash_type = input
//                 .sighash_type
//                 .and_then(|psbt_sighash_type| psbt_sighash_type.taproot_hash_ty().ok())
//                 .unwrap_or(TapSighashType::All);
//             let hash = SighashCache::new(&unsigned_tx).taproot_key_spend_signature_hash(
//                 vout,
//                 &sighash::Prevouts::All(input_txouts.as_slice()),
//                 sighash_type,
//             )?;
//
//             let (_, (_, _derivation_path)) = input
//                 .tap_key_origins
//                 .get(
//                     &input
//                         .tap_internal_key
//                         .ok_or(anyhow!("Internal key missing in PSBT"))?,
//                 )
//                 .ok_or(anyhow!("Missing taproot key origin"))?;
//
//             let secret_key = wallet.account0_xpriv()?.private_key;
//             sign_psbt_taproot(
//                 &secret_key,
//                 input.tap_internal_key.unwrap(),
//                 None,
//                 input,
//                 hash,
//                 sighash_type,
//                 &secp,
//             );
//
//             Ok(())
//         })?;
//
//     bip86_key_finalizer(&mut psbt);
//
//     psbt.serialize_hex().print();
//     // let mut signed_tx = psbt.extract_tx()?;
//     //
//     // for (i, txin) in signed_tx.input.iter_mut().enumerate() {
//     //     if txin.witness.is_empty() {
//     //         txin.witness = new_inputs_completed[i].witness.clone();
//     //     }
//     // }
//
//     // encode::serialize_hex(&signed_tx).print();
//
//     Ok(())
// }
//
// fn sign_psbt_taproot(
//     secret_key: &secp256k1::SecretKey,
//     pubkey: XOnlyPublicKey,
//     leaf_hash: Option<TapLeafHash>,
//     psbt_input: &mut Input,
//     hash: TapSighash,
//     sighash_type: TapSighashType,
//     secp: &Secp256k1<All>,
// ) {
//     let keypair = secp256k1::Keypair::from_seckey_slice(secp, secret_key.as_ref()).unwrap();
//     let keypair = match leaf_hash {
//         None => keypair
//             .tap_tweak(secp, psbt_input.tap_merkle_root)
//             .to_inner(),
//         Some(_) => keypair, // no tweak for script spend
//     };
//
//     let msg = secp256k1::Message::from(hash);
//     let signature = secp.sign_schnorr(&msg, &keypair);
//
//     let final_signature = taproot::Signature {
//         sig: signature,
//         hash_ty: sighash_type,
//     };
//
//     if let Some(lh) = leaf_hash {
//         psbt_input
//             .tap_script_sigs
//             .insert((pubkey, lh), final_signature);
//     } else {
//         psbt_input.tap_key_sig = Some(final_signature);
//     }
// }
//
// fn bip86_key_finalizer(psbt: &mut Psbt) {
//     psbt.inputs.iter_mut().for_each(|input| {
//         if input.final_script_witness.is_none() {
//             let mut script_witness: Witness = Witness::new();
//             script_witness.push(input.tap_key_sig.unwrap().to_vec());
//             input.final_script_witness = Some(script_witness);
//         }
//
//         // Clear all the data fields as per the spec.
//         input.partial_sigs = BTreeMap::new();
//         input.sighash_type = None;
//         input.redeem_script = None;
//         input.witness_script = None;
//         input.bip32_derivation = BTreeMap::new();
//         input.non_witness_utxo = None; //
//     });
// }

// fn key_path_finalizer(psbt: &mut Psbt) {
//     psbt.inputs.iter_mut().for_each(|input| {
//         let mut script_witness: Witness = Witness::new();
//         script_witness.push(input.tap_key_sig.unwrap().to_vec());
//         input.final_script_witness = Some(script_witness);
//
//         // Clear all the data fields as per the spec.
//         input.partial_sigs = BTreeMap::new();
//         input.sighash_type = None;
//         input.redeem_script = None;
//         input.witness_script = None;
//         input.bip32_derivation = BTreeMap::new();
//     });
// }
//
// fn script_path_finalizer(psbt: &mut Psbt) {
//     psbt.inputs.iter_mut().for_each(|input| {
//         let mut script_witness: Witness = Witness::new();
//         for (_, signature) in input.tap_script_sigs.iter() {
//             script_witness.push(signature.to_vec());
//         }
//         for (control_block, (script, _)) in input.tap_scripts.iter() {
//             script_witness.push(script.to_bytes());
//             script_witness.push(control_block.serialize());
//         }
//         input.final_script_witness = Some(script_witness);
//
//         // Clear all the data fields as per the spec.
//         input.partial_sigs = BTreeMap::new();
//         input.sighash_type = None;
//         input.redeem_script = None;
//         input.witness_script = None;
//         input.bip32_derivation = BTreeMap::new();
//         input.tap_script_sigs = BTreeMap::new();
//         input.tap_scripts = BTreeMap::new();
//         input.tap_key_sig = None;
//     });
// }

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use bdk::SignOptions;
    use bip39::Mnemonic;
    use bitcoin::{
        bip32::{DerivationPath, Xpriv},
        hex::FromHex,
        secp256k1::Secp256k1,
        Address, Network,
    };

    use super::*;
    use crate::{
        default,
        wallet::{MnemonicWallet, Mode},
    };

    #[test]
    fn test_() {
        let network = Network::Bitcoin;

        let seed = Mnemonic::from_str("")
            .expect("Invalid mnemonic")
            .to_seed("");
        let secp = Secp256k1::new();

        // let seed =
        //     Vec::from_hex("80668b49e58ac0329c14c1abd9281ce0d6ae338faa5b4e7081bd92327d670a64")
        //         .expect("");
        // let mut buf: Vec<AlignedType> = Vec::new();
        // buf.resize(Secp256k1::preallocate_size(), AlignedType::zeroed());
        // let secp = Secp256k1::preallocated_new(buf.as_mut_slice()).unwrap();

        // 使用种子生成主私钥
        let master_key = Xpriv::new_master(network, &seed).expect("Failed to derive master key");
        let derivation_path =
            DerivationPath::from_str("m/86'/0'/0'/0/0").expect("Invalid derivation path");

        let de = master_key.derive_priv(&secp, &derivation_path).unwrap();

        let public_key = de.private_key.x_only_public_key(&secp).0;

        // 创建 Taproot 地址
        let taproot_address = Address::p2tr(&secp, public_key, None, network);
        println!("{}（{}）", taproot_address, de.to_priv().to_wif());
    }

    #[test]
    fn test_vi() {
        let mut psbt = Psbt::deserialize(&Vec::from_hex("70736274ff0100f302000000024a00299e938d626b0e28025f17d9475dc36535b0222f81765f7acd27b485dcbb0100000000ffffffff1183d5d4ec262e5fccb8c3f146a69235a3a0194a86f4355251ccb6d022b929440900000000ffffffff04093d0000000000002251205eb470e0dfce1da8d410773f6cc40489b48ec78e5b2eaf299be12bb64af3679722020000000000002251201a81e8dd9eb89088bc851041ae6aaa75c3c4e4bc43454821e26003e68af9495d00000000000000000d6a5d0a00c0a233970392f40101e10e0100000000002251201a81e8dd9eb89088bc851041ae6aaa75c3c4e4bc43454821e26003e68af9495d000000000001012b5e010000000000002251205eb470e0dfce1da8d410773f6cc40489b48ec78e5b2eaf299be12bb64af36797011720613280f8c7fff7b128b81c15845bae82ad612e6b4b5210b024fe7d4c624eaf4a0001012b22610100000000002251201a81e8dd9eb89088bc851041ae6aaa75c3c4e4bc43454821e26003e68af9495d0117209bd68b2123e2d08e923f637d80a66240be5e1bd0b876a01b874f3f7a4a1650dd0000000000").unwrap()).unwrap();

        let wallet = MnemonicWallet::new("", Mode::Other, Network::Bitcoin).unwrap();

        let ok = wallet.sign(&mut psbt).unwrap();
        println!("{}", ok);
        let f = wallet
            .pay_wallet
            .finalize_psbt(
                &mut psbt,
                SignOptions {
                    trust_witness_utxo: true,
                    ..default()
                },
            )
            .unwrap();

        let signed_tx = psbt.clone().extract_tx().unwrap();
        let hex = encode::serialize_hex(&signed_tx);

        println!("{}", f);
        println!("{}", hex);
    }

    #[test]
    fn test_gen_addr() {
        let addr =
            Address::from_str("bc1pyf5f0r5eqxer5rdrwm98grgz5tem6k8xgtnm49he2m4kjhacrsms6p6888")
                .unwrap()
                .assume_checked();

        addr.script_pubkey().script_hash().print();
    }
}
