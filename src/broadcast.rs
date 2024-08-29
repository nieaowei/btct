use std::collections::HashMap;

use reqwest::header::{HeaderMap, CONTENT_TYPE};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct RestApi {
    pub(crate) api_addr: String,
    /*
       text: $tx
       json: { "hex": "$tx" }
    */
    body_format: String,
}

fn broadcast(tx_hex: &str, format: &RestApi) -> anyhow::Result<String> {
    let params = format.body_format.replace("$tx", tx_hex);

    let cli = reqwest::blocking::Client::new();
    // if  {  }
    let mut header = HeaderMap::new();
    if let Ok(_) = serde_json::from_str::<'_, HashMap<String, serde_json::Value>>(&params) {
        header.insert(CONTENT_TYPE, "application/json".parse()?);
    }

    let resp = cli.post(&format.api_addr).body(params).send()?;

    Ok(resp.text()?)
}

impl RestApi {
    pub(crate) fn broadcast(&self, tx_hex: &str) -> anyhow::Result<String> {
        broadcast(tx_hex, &self)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[test]
    fn test() {
        if let Ok(_) =
            serde_json::from_str::<'_, HashMap<String, serde_json::Value>>("{\"rawhex\": \"$tx\"}")
        {
            println!("ok json")
        }

        if let Ok(_) = serde_json::from_str::<'_, HashMap<String, serde_json::Value>>("$tx") {
            println!("ok")
        }
    }
}
