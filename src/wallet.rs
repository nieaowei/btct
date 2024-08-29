use std::str::FromStr;

use bdk::{
    chain::{local_chain::CheckPoint, spk_client::SyncRequest},
    keys::{DerivableKey, ExtendedKey},
    template, KeychainKind, SignOptions,
};
use bip39::Mnemonic;
use bitcoin::Psbt;
use miniscript::{
    bitcoin::{
        secp256k1::{All, Secp256k1},
        Address, Network,
    },
    Tap, ToPublicKey,
};
use serde::Deserialize;

use crate::default;

#[derive(Debug, Deserialize, Default, Copy, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mode {
    #[default]
    Other,
    XVerse,
    Peek,
}

pub(crate) struct MnemonicWallet {
    pub(crate) pay_wallet: bdk::Wallet,
    ordi_wallet: bdk::Wallet,
    mode: Mode,
    network: Network,
}

impl MnemonicWallet {
    pub(crate) fn new(mnemonic: &str, mode: Mode, network: Network) -> anyhow::Result<Self> {
        match mode {
            Mode::Other => Self::new_mnemonic_with_mode(mnemonic, network, mode),
            Mode::XVerse => Self::new_mnemonic_by_xverse(mnemonic, network),
            Mode::Peek => Self::new_mnemonic_with_mode(mnemonic, network, mode),
        }
    }

    pub(crate) fn check(&self) {
        log::info!("[wallet] PayAddr: {} ", self.pay_addr().to_string());
        log::info!("[wallet] OrdiAddr: {} ", self.ordi_addr().to_string());
    }

    pub(crate) fn new_mnemonic_with_mode(
        mnemonic: &str,
        network: Network,
        mode: Mode,
    ) -> anyhow::Result<Self> {
        let mnemonic = Mnemonic::parse(mnemonic)?;
        let xkey: ExtendedKey<Tap> = mnemonic.into_extended_key().unwrap();
        let xprv = xkey.into_xprv(network).unwrap();

        let ordi = bdk::Wallet::new(
            template::Bip86(xprv, KeychainKind::External),
            None,
            (),
            network,
        )?;

        let pay = bdk::Wallet::new(
            template::Bip86(xprv, KeychainKind::External),
            None,
            (),
            network,
        )?;

        return Ok(Self {
            pay_wallet: pay,
            ordi_wallet: ordi,
            mode,
            network,
        });
    }

    pub(crate) fn new_mnemonic_by_xverse(mnemonic: &str, network: Network) -> anyhow::Result<Self> {
        let mnemonic = Mnemonic::parse(mnemonic)?;
        let xkey: ExtendedKey<Tap> = mnemonic.into_extended_key().unwrap();
        let xprv = xkey.into_xprv(network).unwrap();

        let ordi = bdk::Wallet::new_no_persist(
            template::Bip86(xprv, KeychainKind::External),
            None,
            network,
        )?;

        let pay = bdk::Wallet::new_no_persist(
            template::Bip49(xprv, KeychainKind::External),
            None,
            network,
        )?;

        return Ok(Self {
            pay_wallet: pay,
            ordi_wallet: ordi,
            mode: Mode::XVerse,
            network,
        });
    }

    pub(crate) fn pay_addr(&self) -> Address {
        self.pay_wallet
            .peek_address(KeychainKind::External, 0)
            .address
    }

    pub(crate) fn peek_addr(&self, peek: u32) -> Address {
        self.pay_wallet
            .peek_address(KeychainKind::External, peek)
            .address
    }
    pub(crate) fn ordi_addr(&self) -> Address {
        match self.mode {
            Mode::Peek => {
                self.ordi_wallet
                    .peek_address(KeychainKind::External, 1)
                    .address
            }
            _ => {
                self.ordi_wallet
                    .peek_address(KeychainKind::External, 0)
                    .address
            }
        }
    }

    pub(crate) fn sign_with_options(
        &self,
        psbt: &mut Psbt,
        sign_options: SignOptions,
    ) -> anyhow::Result<bool> {
        Ok(self.pay_wallet.sign(psbt, sign_options)?)
    }

    pub(crate) fn sign(&self, psbt: &mut Psbt) -> anyhow::Result<bool> {
        self.sign_with_options(
            psbt,
            SignOptions {
                trust_witness_utxo: true,
                ..default()
            },
        )
    }

    pub(crate) fn sign_swap(&self, psbt: &mut Psbt) -> anyhow::Result<bool> {
        Ok(self.ordi_wallet.sign(
            psbt,
            SignOptions {
                trust_witness_utxo: true,
                allow_all_sighashes: true,
                ..default()
            },
        )?)
    }
    pub(crate) fn ctx(&self) -> &Secp256k1<All> {
        self.pay_wallet.secp_ctx()
    }
}

pub trait Wallet {
    fn pay_addr(&mut self) -> Address;
    fn ordi_addr(&mut self) -> Address;
    fn ctx(&self) -> &Secp256k1<All>;
    fn sign(&self, psbt: &mut Psbt) -> anyhow::Result<bool>;
}

//
// pub(crate) struct PrivateKeyWallet {
//     wif: String,
//     pay_addr: String,
//     secp256k1: Secp256k1<All>,
// }
//
// impl PrivateKeyWallet {
//     pub(crate) fn new(pk_wif: &str, pay_addr: &str) -> Self {
//         Self {
//             wif: pk_wif.to_string(),
//             pay_addr: pay_addr.to_string(),
//             secp256k1: Secp256k1::new(),
//         }
//     }
//
//     pub(crate) fn private_key(&self) -> anyhow::Result<PrivateKey> {
//         Ok(PrivateKey::from_wif(&self.wif)?)
//     }
//
//     pub(crate) fn public_key(&self) -> anyhow::Result<PublicKey> {
//         let pk = self.private_key()?;
//         Ok(pk.public_key(&Secp256k1::new()))
//     }
//     pub(crate) fn private_keys(&self) -> anyhow::Result<HashMap<PublicKey, PrivateKey>> {
//         let mut map = HashMap::new();
//         let pk = PrivateKey::from_wif(&self.wif)?;
//         map.insert(pk.public_key(&Secp256k1::new()), pk);
//         Ok(map)
//     }
//
//     pub(crate) fn sign_psbt(&self, psbt: &mut Psbt) -> anyhow::Result<()> {
//         let signingkeys = psbt.sign(&self.private_keys()?, &Secp256k1::new()).unwrap();
//         for (index, signingkey) in signingkeys {
//             println!("[{index}] : [{signingkey:?}]")
//         }
//         Ok(())
//     }
// }
//
// impl Wallet for PrivateKeyWallet {
//     fn sign_psbt(&self, psbt: &mut Psbt) -> anyhow::Result<()> {
//         self.sign_psbt(psbt)
//     }
//
//     fn pay_addr(&self) -> anyhow::Result<Address> {
//         Ok(Address::from_str(&self.pay_addr)?.assume_checked())
//     }
//
//     fn public_key(&self) -> anyhow::Result<PublicKey> {
//         self.public_key()
//     }
//
//     fn secret_key(&self) -> anyhow::Result<SecretKey> {
//         Ok(self.private_key()?.inner)
//     }
//
//     fn xpriv(&self) -> anyhow::Result<Xpriv> {
//         unreachable!()
//     }
//
//     fn secp(&self) -> &Secp256k1<All> {
//         &self.secp256k1
//     }
// }
//
// pub(crate) struct MnemonicWallet {
//     network: Network,
//     pub(crate) derivation_path: DerivationPath,
//     pub(crate) master_xpriv: Xpriv,
//     pub(crate) master_xpub: Xpub,
//     pass: Option<String>,
//     pub(crate) secp: Secp256k1<All>,
// }
//
// impl MnemonicWallet {
//     pub(crate) fn new(words: &str, network: Network) -> anyhow::Result<Self> {
//         let secp = Secp256k1::new();
//         let master_xpriv = Self::master_xpriv(network, words, None)?;
//         let master_xpub = Xpub::from_priv(&secp, &master_xpriv);
//         let derivation_path = DerivationPath::from_str("m/86'/0'/0'/0/0")?;
//         Ok(Self {
//             network,
//             master_xpriv,
//             master_xpub,
//             pass: None,
//             secp,
//             derivation_path,
//         })
//     }
//     pub(crate) fn master_xpriv(
//         network: Network,
//         words: &str,
//         pass: Option<&str>,
//     ) -> anyhow::Result<Xpriv> {
//         let mnemonic = Mnemonic::parse(words)?;
//         let pass = pass.unwrap_or_default();
//         let seed = mnemonic.to_seed(pass);
//         let master_key = Xpriv::new_master(network, &seed)?;
//         Ok(master_key)
//     }
//
//     pub(crate) fn account0_xpriv(&self) -> anyhow::Result<Xpriv> {
//         let de = self
//             .master_xpriv
//             .derive_priv(&self.secp, &self.derivation_path)?;
//         Ok(de)
//     }
//
//     pub(crate) fn account0_xpub(&self) -> anyhow::Result<Xpub> {
//         Ok(Xpub::from_priv(&self.secp, &(self.account0_xpriv()?)))
//     }
//
//     pub(crate) fn account0_addr(&self) -> anyhow::Result<Address> {
//         Ok({
//             Address::p2tr(
//                 &self.secp,
//                 self.account0_xpub()?.public_key.to_x_only_pubkey(),
//                 None,
//                 self.network,
//             )
//         })
//     }
//
//     pub(crate) fn master_fingerprint(&self) -> Fingerprint {
//         self.master_xpub.fingerprint()
//     }
// }

//
// impl Wallet for MnemonicWallet {
//     fn sign_psbt(&self, psbt: &mut Psbt) -> anyhow::Result<()> {
//         self.sign_psbt(psbt)
//     }
//
//     fn pay_addr(&self) -> anyhow::Result<Address> {
//         Ok(Address::from_str(&self.pay_addr)?.require_network(self.network)?)
//     }
//
//     fn public_key(&self) -> anyhow::Result<PublicKey> {
//         unreachable!()
//     }
//
//     fn secret_key(&self) -> anyhow::Result<SecretKey> {
//         unreachable!()
//     }
//
//     fn xpriv(&self) -> anyhow::Result<Xpriv> {
//         self.xpriv()
//     }
//
//     fn secp(&self) -> &Secp256k1<All> {
//         &self.secp256k1
//     }
// }
