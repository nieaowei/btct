use std::{io::Read, path::PathBuf, str::FromStr};

use anyhow::bail;
use bitcoin::{
    absolute::LockTime, psbt::Input, Address, Amount, FeeRate, OutPoint, Psbt, Sequence,
    Transaction, TxIn, TxOut,
};
use btct::{
    cancel::cancel,
    default,
    monitor::monitor,
    send::send,
    setting::{read_settings_from_file, Settings, SettingsSerde},
    snipe::{snipe, Type},
    speed_up::speed_up,
};
use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use env_logger::Env;
use serde::Serialize;

#[derive(Parser)]
#[command(version, about, long_about = None)]
/// Bitcoin Tools
struct App {
    #[arg(short, long, default_value = "./config.toml")]
    /// Custom config path
    config: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Parser)]
#[command(group(
    ArgGroup::new("txid_or_addr")
    .args(&["txid", "addr"])
    .required(true),
))]
struct TxidOrAddr {
    #[arg(long)]
    txid: String,
    #[arg(long)]
    addr: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Replace other tx in mempool
    Snipe {
        // #[command(flatten)]
        // txid_or_addr: TxidOrAddr,
        #[arg(long, group = "txid_or_addr")]
        txid: Option<String>,
        #[arg(long, group = "txid_or_addr")]
        addr: Option<String>,
        #[arg(short, long, default_value = "auto")]
        typ: Type,
        // #[arg(short, long)]
        // fee_rate: u64,
        #[arg(short, long)]
        /// [increase] than origin tx fee
        increase_rate: u64,
        #[arg(short = 'b', long, default_value_t = false)]
        broadcast: bool,
        #[arg(long = "show", default_value_t = false)]
        show_tx: bool,
        #[arg(short, long, default_value_t = false)]
        /// Skip replace confirm
        yes: bool,
        #[arg(short, long, default_value_t = false)]
        poison: bool,
        #[arg(short, long, default_value_t = false)]
        simple: bool,
        #[arg(long)]
        /// Split tx rate under simple mode
        split_rate: Option<u64>,
        #[arg(long)]
        split_recv: Option<String>,
        #[arg(long)]
        /// Check seller outpoint
        check: Option<String>,
        /// monitor
        #[arg(short, long, default_value_t = true)]
        monitor: bool,
    },
    /// Speed up unconfirmed tx by <RBF> or <CPFP>
    SpeedUp {
        #[arg(short, long)]
        tx_id: String,
        #[arg(short, long, default_value_t = 10)]
        increase_rate: u64,
        #[arg(short = 'b', long, default_value_t = false)]
        broadcast: bool,
    },
    /// Prepare your wallet, generate <number> UTXO of <amount>
    Prepare {
        #[arg(short, long, default_value_t = 6)]
        number: u64,
    },
    /// Cancel unconfirmed tx
    Cancel {
        #[arg(short, long, default_value_t = 10)]
        /// [increase_fee] than origin tx
        increase_fee: u64,
        #[arg(short, long, default_value_t = 600)]
        /// Dummy utxo size
        dummy_utxo: u64,
        #[arg(short, long, default_value_t = 546)]
        /// If tx contain inscription or rune, please setting
        postage: u64,
        #[arg(short, long, default_value_t = false)]
        /// Don't collect dummy utxo
        origin: bool,
        #[arg(short, long, default_value_t = 0)]
        peek: u64,
        #[arg(long)]
        cancel_addr: Option<String>,
    },
    /// Send btc, inscription and runes address
    Send {
        #[arg(long)]
        addr: String,
        #[arg(short, long)]
        fee_rate: u64,
        #[arg(short, long)]
        amount: f64,
        #[arg(short = 'b', long, default_value_t = false)]
        broadcast: bool,
    },
    /// Check setting and wallet
    Monitor {
        #[arg(long)]
        txid: String,
        #[arg(short, default_value_t = 3)]
        interval: u64,
    },
    Check {},
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_target(false)
        .init();

    let app = App::parse();

    let settings = read_settings_from_file(app.config)?;

    match app.command {
        Commands::Snipe {
            txid: tx_id,
            addr,
            typ,
            increase_rate,
            broadcast,
            show_tx,
            yes,
            poison,
            simple,
            split_rate,
            check,
            monitor,
            split_recv,
        } => {
            if let Err(err) = snipe(
                settings,
                &tx_id.unwrap_or_default(),
                &addr.unwrap_or_default(),
                typ,
                increase_rate,
                broadcast,
                show_tx,
                yes,
                poison,
                simple,
                split_rate,
                split_recv,
                check,
                monitor,
            ) {
                log::error!("{}", err);
            }
        }
        Commands::Check {} => {
            settings.check()?;
        }
        Commands::SpeedUp {
            tx_id,
            increase_rate,
            broadcast,
        } => {
            // test(&settings, &tx_id)?;
            // speed_up(settings, &tx_id, increase_rate, broadcast)?;
        }
        Commands::Prepare { number } => {}
        Commands::Cancel {
            increase_fee: increase_rate,
            postage,
            dummy_utxo,
            origin,
            peek,
            cancel_addr,
        } => {
            cancel(
                settings,
                cancel_addr,
                increase_rate,
                postage,
                dummy_utxo,
                origin,
                peek,
            )?;
        }
        Commands::Send {
            addr,
            amount,
            fee_rate,
            broadcast,
        } => {
            send(settings, &addr, amount, fee_rate, broadcast)?;
        }
        Commands::Monitor { txid, interval } => {
            monitor(&settings, &txid, interval);
        }
    }
    Ok(())
}

// fn test(settings: &Settings, txid: &str) -> anyhow::Result<()> {
//     let api = settings.btc_api();
//     let wallet = settings.wallet()?;
//     let addr = wallet.peek_addr(1);
//     let mut tx = api.get_transaction(txid)?;
//     log::info!("{}", addr);
//
//     tx.vout.last_mut().unwrap().value -= Amount::from_sat(1000);
//
//     let mut psbt = Psbt {
//         unsigned_tx: Transaction {
//             version: tx.version,
//             lock_time: LockTime::from_consensus(tx.locktime),
//             input: tx
//                 .vin
//                 .iter()
//                 .map(|e| TxIn {
//                     previous_output: OutPoint {
//                         txid: e.txid,
//                         vout: e.vout,
//                     },
//                     script_sig: Default::default(),
//                     sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
//                     witness: Default::default(),
//                 })
//                 .collect(),
//             output: tx
//                 .vout
//                 .iter()
//                 .map(|e| TxOut {
//                     value: e.value,
//                     script_pubkey: Address::from_str(&e.scriptpubkey_address)
//                         .unwrap()
//                         .assume_checked()
//                         .script_pubkey(),
//                 })
//                 .collect(),
//         },
//         version: 0,
//         xpub: Default::default(),
//         proprietary: Default::default(),
//         unknown: Default::default(),
//         inputs: tx
//             .vin
//             .iter()
//             .map(|e| Input {
//                 witness_utxo: Some({
//                     TxOut {
//                         value: e.prevout.value,
//                         script_pubkey: addr.script_pubkey(),
//                     }
//                 }),
//                 ..default()
//             })
//             .collect(),
//         outputs: vec![default(); 2],
//     };
//
//     psbt.serialize_hex().print();
//
//     let ok = wallet.sign(&mut psbt)?;
//     if !ok {
//         bail!("adsd")
//     }
//
//     psbt.serialize_hex().print();
//
//     Ok(())
// }
