use bitcoin::Amount;

pub(crate) const ADDITIONAL_INPUT_VBYTES: usize = 58;
pub(crate) const ADDITIONAL_OUTPUT_VBYTES: usize = 43;
pub(crate) const SCHNORR_SIGNATURE_SIZE: usize = 64;

pub(crate) const POSTAGE: Amount = Amount::from_sat(546);

pub(crate) const DUMMY_UTXO: Amount = Amount::from_sat(600);

pub(crate) const MIN_UTXO: Amount = Amount::from_sat(10000);

pub(crate) const APPEND_NETWORK_FEE_SAT: Amount = Amount::from_sat(666);
