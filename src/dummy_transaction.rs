use bitcoin::{
    absolute::LockTime,
    Address,
    AddressType,
    Amount, key::constants::SCHNORR_SIGNATURE_SIZE, OutPoint, ScriptBuf, Sequence, Transaction, transaction::Version, TxIn, TxOut, Witness,
};

pub(crate) struct DummyTransaction(pub Transaction);

impl DummyTransaction {
    pub(crate) fn new() -> Self {
        DummyTransaction(Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![],
            output: vec![],
        })
    }

    pub(crate) fn append_input(
        &mut self,
        addr: Address,
        sig: Option<ScriptBuf>,
        witness: Option<Witness>,
    ) {
        let sig = sig.unwrap_or({
            match addr.address_type().unwrap() {
                AddressType::P2sh => addr.script_pubkey().to_p2sh(),
                AddressType::P2wsh => addr.script_pubkey().to_p2wsh(),
                _ => ScriptBuf::new(),
            }
        });
        let witness = witness.unwrap_or({
            if addr.script_pubkey().is_p2tr() {
                Witness::from_slice(&[&[0; SCHNORR_SIGNATURE_SIZE]])
            } else {
                Witness::from_slice(&[vec![0; 71], vec![0; 33]]) // 第一个值最大73 这里已知在xx下
            }
        });
        self.0.input.push(TxIn {
            previous_output: OutPoint::null(),
            script_sig: sig,
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: witness,
        })
    }

    pub(crate) fn append_output(&mut self, script_pubkey: ScriptBuf) {
        self.0.output.push(TxOut {
            value: Amount::ZERO,
            script_pubkey: script_pubkey,
        })
    }

    pub(crate) fn vsize(&self) -> usize {
        self.0.vsize()
    }
}
