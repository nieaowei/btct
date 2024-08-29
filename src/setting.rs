use std::{collections::HashSet, fs::File, io::Read, path::PathBuf};

use anyhow::{anyhow, bail};
use bdk::{
    keys::{DerivableKey, ExtendedKey},
    template::Bip86,
    KeychainKind,
};
use bip39::Mnemonic;
use miniscript::{bitcoin::Network, Tap};
use serde::Deserialize;

use crate::{
    broadcast, btc_api,
    btc_api::ordinal,
    wallet::{MnemonicWallet, Mode, Wallet},
};

#[derive(Deserialize)]
pub struct SettingsSerde {
    network: Network,
    private_key: Option<String>,
    mnemonic: Option<String>,
    wallet_mode: Mode,
    //污点
    poison_mnemonic: Option<String>,
    ordi_api: String,
    rpc_api: Option<String>,
    broadcast_rest_apis: Vec<broadcast::RestApi>,
}

pub struct Settings {
    // pub(crate) btc_api_addr: Address<NetworkChecked>,
    pub(crate) network: Network,
    // pub(crate) private_key: Option<String>,
    pub(crate) mnemonic: Option<String>,
    pub(crate) poison_mnemonic: Option<String>,
    wallet_mode: Mode,
    ordi_api: String,
    rpc_api: Option<String>,
    broadcast_rest_apis: Vec<broadcast::RestApi>,
}

impl TryFrom<SettingsSerde> for Settings {
    type Error = anyhow::Error;

    fn try_from(value: SettingsSerde) -> Result<Self, Self::Error> {
        Ok(Self {
            network: value.network,
            // private_key: value.private_key,
            mnemonic: value.mnemonic,
            poison_mnemonic: value.poison_mnemonic,
            wallet_mode: value.wallet_mode,
            ordi_api: value.ordi_api,
            rpc_api: value.rpc_api,
            broadcast_rest_apis: value.broadcast_rest_apis,
        })
    }
}

pub fn read_settings_from_file(path_buf: PathBuf) -> anyhow::Result<Settings> {
    let mut settings_str = String::new();
    let mut file = File::open(path_buf).expect("Open file failed");
    let _res = file.read_to_string(&mut settings_str).expect("");

    let settings: Settings = toml::from_str::<SettingsSerde>(&settings_str)
        .expect("Config content must be [toml]")
        .try_into()?;
    Ok(settings)
}

impl Settings {
    pub fn check(&self) -> anyhow::Result<()> {
        let wallet = self.wallet()?;
        let poison_wallet = self.poison_wallet()?;
        log::info!("[Network] {} ", self.network);
        log::info!("[Wallet] Mode: {:?} ", self.wallet_mode);
        log::info!("[Wallet] Pay: {} ", wallet.pay_addr());
        log::info!("[Wallet] Ordi: {} ", wallet.ordi_addr());
        log::info!("[PoisonWallet] Pay: {} ", poison_wallet.pay_addr());
        log::info!("[PoisonWallet] Ordi: {} ", poison_wallet.ordi_addr());
        Ok(())
    }

    pub(crate) fn rpc_api(&self) -> Option<btc_api::btc_json_rpc::Client> {
        Some(btc_api::btc_json_rpc::Client::new(
            self.rpc_api.as_ref()?,
            60,
        ))
    }

    pub(crate) fn btc_api(&self) -> btc_api::esplora::Client {
        btc_api::esplora::new(self.network)
    }

    pub(crate) fn ordi_api(&self) -> ordinal::Client {
        // ordinal::Client::new("https://javirbin.com")
        ordinal::Client::new(&self.ordi_api)
    }

    pub(crate) fn wallet(&self) -> anyhow::Result<MnemonicWallet> {
        if let Some(words) = &self.mnemonic {
            return Ok(MnemonicWallet::new(words, self.wallet_mode, self.network)?);
        }
        bail!("Please setting [mnemonic]")
    }

    pub(crate) fn poison_wallet(&self) -> anyhow::Result<MnemonicWallet> {
        if let Some(words) = &self.poison_mnemonic {
            return Ok(MnemonicWallet::new(words, Mode::XVerse, self.network)?);
        }
        bail!("Please setting [poison_mnemonic]")
    }

    pub(crate) fn broadcast(&self, tx_hex: &str) -> anyhow::Result<()> {
        for broadcast_rest_api in self.broadcast_rest_apis.iter() {
            let res = broadcast_rest_api.broadcast(tx_hex)?;
            log::info!("[broadcast] {} : {}", broadcast_rest_api.api_addr, res);
        }
        Ok(())
    }

    pub(crate) fn config_init(&self) {}
}

#[cfg(test)]
mod tests {
    use env_logger::Env;

    use super::*;

    #[test]
    fn test_wallet() {
        env_logger::Builder::from_env(Env::default().default_filter_or("info"))
            .format_target(false)
            .init();
        let tx_hex = "02000000000101440dda6247dbc0fca4d4583eba57575ff51cdad6618ea2f94229f66cfb3bee160100000000fdffffff020000000000000000076a5d04140114009e270b0000000000160014d30aeee179daaa9b65e8dda50d888bff6c38023202483045022100f7df4d1d48aeaf2ec85679874ba76c508e32217551e9ee024d5a0e146a78bf27022052e35bb5bb61947129557ca419945aadcd08e8253825bec38a411d0fc5535556012102211c1f042288a40cc5db40b3db825a9224ad0f9d0346e17a373a3beffacd98ee00000000";
        let settings = read_settings_from_file(("./dev.toml".parse().unwrap())).unwrap();
        settings.broadcast(&tx_hex).unwrap();
    }
}
