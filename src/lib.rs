use serde::Serialize;

pub(crate) mod btc_api;
pub mod setting;
pub mod snipe;

pub mod constant;

pub mod wallet;

pub(crate) mod rune;

pub mod broadcast;

pub mod check;
pub(crate) mod dummy_transaction;

pub(crate) mod utils;

pub mod prepare;

pub mod cancel;
mod demo;
pub mod send;
pub mod speed_up;

pub(crate) mod error;
pub mod monitor;

pub mod audio;

pub mod server;

pub fn default<T: Default>() -> T {
    Default::default()
}

pub trait Print {
    fn print(&self);
}

impl<T> Print for T
where
    T: Serialize,
{
    fn print(&self) {
        println!("{}", serde_json::to_string(self).expect(""));
    }
}
