use std::str::FromStr;

use anyhow::{anyhow, bail};
use bitcoin::{Amount, OutPoint};
use ordinals::{Rune, RuneId};
use regex::Regex;
use reqwest::header::ACCEPT;
use scraper::{html, Element, Selector};
use serde::{Deserialize, Serialize};

use crate::Print;

#[derive(Debug, Eq, PartialEq, Clone)]
pub(crate) enum Ordinal {
    None,
    Inscription {
        id: String,
        value: Amount,
        out_point: OutPoint,
    },
    Rune {
        id: RuneId,
        name: String,
        value: Amount,
        number: u128,
        div: u32,
        out_point: OutPoint,
    },
}

impl Ordinal {
    pub(crate) fn is_none(&self) -> bool {
        self == &Self::None
    }

    pub(crate) fn is_rune(&self) -> bool {
        let Ordinal::Rune { .. } = self else {
            return false;
        };
        return true;
    }

    pub(crate) fn is_inscription(&self) -> bool {
        let Ordinal::Inscription { .. } = self else {
            return false;
        };
        return true;
    }

    pub(crate) fn outpoint(&self) -> Option<OutPoint> {
        match self {
            Ordinal::None => None,
            Ordinal::Inscription { out_point, .. } => Some(out_point.clone()),
            Ordinal::Rune { out_point, .. } => Some(out_point.clone()),
        }
    }

    pub(crate) fn display(&self) -> String {
        match self {
            Ordinal::None => {
                format!("")
            }
            Ordinal::Inscription {
                id,
                value,
                out_point,
            } => {
                format!("{}\nInscription {}", value, id,)
            }
            Ordinal::Rune {
                id,
                name,
                value,
                number,
                out_point,
                ..
            } => {
                format!("{}\nRune {} {}", value, name, number,)
            }
        }
    }

    pub(crate) fn display_value(&self, sell_value: Amount) -> String {
        match self {
            Ordinal::None => {
                format!("")
            }
            Ordinal::Inscription { id, .. } => {
                format!("Inscription {} {}", id, sell_value)
            }
            Ordinal::Rune { name, number, .. } => {
                format!(
                    "Rune {} {} {:.2} sat/unit",
                    name,
                    number,
                    sell_value.to_sat() as f64 / *number as f64
                )
            }
        }
    }
}

pub(crate) fn fetch_outputs(utxo: Vec<&OutPoint>) -> anyhow::Result<Vec<(usize, Ordinal)>> {
    let mut ordis = Vec::new();
    for (id, out_point) in utxo.into_iter().enumerate() {
        let ordi = fetch_output(out_point)?;
        if !ordi.is_none() {
            ordis.push((id, ordi));
        }
    }
    Ok(ordis)
}

pub(crate) fn fetch_output(out_point: &OutPoint) -> anyhow::Result<Ordinal> {
    let html =
        reqwest::blocking::get(format!("https://ordinals.com/output/{}", out_point))?.text()?;
    let html = html::Html::parse_document(&html);

    let select_title = Selector::parse("main dl dt").unwrap();
    let title_select = html
        .select(&select_title)
        .next()
        .ok_or(anyhow!("Not found <title>"))?;
    let title = title_select.text().collect::<String>();

    Ok({
        match title.as_str() {
            "inscriptions" => {
                let content = title_select
                    .next_sibling_element()
                    .ok_or(anyhow!("Not found title content"))?;
                let href_select = Selector::parse("a").unwrap();
                let href = content.select(&href_select).next().unwrap();
                let id = href
                    .attr("href")
                    .unwrap()
                    .trim_start_matches("/inscription/")
                    .to_string();
                let value = content
                    .next_sibling_element()
                    .ok_or(anyhow!("No value title"))?
                    .next_sibling_element()
                    .ok_or(anyhow!("No value number"))?
                    .text()
                    .collect::<String>();
                Ordinal::Inscription {
                    id,
                    value: Amount::from_sat(value.parse()?),
                    out_point: out_point.clone(),
                }
            }
            "runes" => {
                let content = title_select.next_sibling_element().unwrap();
                let td_select = Selector::parse("td").unwrap();
                let tds = content.select(&td_select).collect::<Vec<_>>();
                let name = tds[0].text().collect::<String>();
                let number = tds[1].text().collect::<String>();

                let number = Regex::new(r#"\d+"#)
                    .unwrap()
                    .find_iter(&number)
                    .map(|e| e.as_str())
                    .collect::<Vec<&str>>()[0];

                let value = content
                    .next_sibling_element()
                    .ok_or(anyhow!("No value title"))?
                    .next_sibling_element()
                    .ok_or(anyhow!("No value number"))?
                    .text()
                    .collect::<String>();

                let (rune_id, div) = fetch_rune_id(&name)?;
                Ordinal::Rune {
                    id: rune_id,
                    name,
                    value: Amount::from_sat(value.parse()?),
                    number: number.parse()?,
                    div,
                    out_point: out_point.clone(),
                }
            }
            _ => Ordinal::None,
        }
    })
}
//
pub(crate) fn fetch_rune_id(name: &str) -> anyhow::Result<(RuneId, u32)> {
    let html = reqwest::blocking::get(format!("https://ordinals.com/rune/{}", name))?.text()?;

    let html = html::Html::parse_document(&html);

    let select_title = Selector::parse("main dl dt").unwrap();
    let title_select = html.select(&select_title);

    let mut title_selected = None;
    let mut div_selected = None;
    for title_e in title_select {
        let title = title_e.text().collect::<String>();
        if title == "id" {
            title_selected = Some(title_e);
        }
        if title == "divisibility" {
            div_selected = Some(title_e);
        }
    }
    let Some(title_selected) = title_selected else {
        bail!("Not found rune title");
    };
    let id = title_selected
        .next_sibling_element()
        .unwrap()
        .text()
        .collect::<String>();

    let Some(div_selected) = div_selected else {
        bail!("Not found rune div");
    };
    let div = div_selected
        .next_sibling_element()
        .unwrap()
        .text()
        .collect::<String>();
    Ok((RuneId::from_str(id.as_str())?, div.parse()?))
}

pub struct Client {
    addr: String,
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new(addr: &str) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(ACCEPT, "application/json".parse().unwrap());
        let http = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();
        Self {
            addr: addr.to_string(),
            http,
        }
    }
    pub(crate) fn fetch_outputs(
        &self,
        utxo: Vec<&OutPoint>,
    ) -> anyhow::Result<Vec<(usize, Ordinal)>> {
        let mut ordis = Vec::new();
        for (id, out_point) in utxo.into_iter().enumerate() {
            let ordi = self.fetch_output(out_point)?;
            if !ordi.is_none() {
                ordis.push((id, ordi));
            }
        }
        Ok(ordis)
    }

    pub(crate) fn fetch_one_rune_output(&self, utxo: Vec<&OutPoint>) -> anyhow::Result<Ordinal> {
        for (id, out_point) in utxo.into_iter().enumerate() {
            let ordi = self.fetch_output(out_point)?;
            if ordi.is_rune() {
                return Ok(ordi);
            }
        }
        bail!("No inscription or rune in output")
    }

    pub(crate) fn fetch_one_inscription_output(
        &self,
        utxo: Vec<&OutPoint>,
    ) -> anyhow::Result<Ordinal> {
        for (id, out_point) in utxo.into_iter().enumerate() {
            let ordi = self.fetch_output(out_point)?;
            if ordi.is_inscription() {
                return Ok(ordi);
            }
        }
        bail!("No inscription or rune in output")
    }
    pub fn fetch_output(&self, out_point: &OutPoint) -> anyhow::Result<Ordinal> {
        let mut o = self
            .http
            .get(format!("{}/output/{}", self.addr, out_point))
            .send()?
            .json::<Output>()?;
        if !o.inscriptions.is_empty() {
            let inscription = o.inscriptions.pop().unwrap();

            return Ok(Ordinal::Inscription {
                id: inscription,
                value: o.value,
                out_point: out_point.clone(),
            });
        }
        if !o.runes.is_empty() {
            let rune = o.runes.pop().unwrap();
            let RuneItem::Name(name) = &rune[0] else {
                bail!("error name")
            };
            let RuneItem::Info {
                amount,
                divisibility,
                symbol,
            } = &rune[1]
            else {
                bail!("error name")
            };
            let rune = self.fetch_rune_id(name)?;
            return Ok(Ordinal::Rune {
                id: rune.id.parse()?,
                name: name.to_string(),
                value: o.value,
                number: *amount as u128 / 10u128.pow(*divisibility),
                div: *divisibility,
                out_point: out_point.clone(),
            });
        }
        Ok(Ordinal::None)
    }

    pub fn fetch_rune_id(&self, name: &str) -> anyhow::Result<RuneEntity> {
        let o = self
            .http
            .get(format!("{}/rune/{}", self.addr, name))
            .send()?
            .json::<RuneEntity>()?;
        Ok(o)
    }
}

#[derive(Serialize, Deserialize)]
pub struct RuneEntry {
    pub block: u64,
    pub burned: u64,
    pub divisibility: u32,
    pub etching: String,
    pub mints: u64,
    pub number: u64,
    pub premine: u64,
    pub spaced_rune: String,
    pub symbol: String,
    pub timestamp: i64,
    pub turbo: bool,
}

#[derive(Serialize, Deserialize)]
pub struct RuneEntity {
    pub entry: RuneEntry,
    pub id: String,
    pub mintable: bool,
    pub parent: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Output {
    pub address: String,
    pub indexed: bool,
    pub inscriptions: Vec<String>,
    pub runes: Vec<Vec<RuneItem>>,
    pub script_pubkey: String,
    pub spent: bool,
    pub transaction: String,
    pub value: Amount,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuneItem {
    Name(String),
    Info {
        amount: u64,
        divisibility: u32,
        symbol: String,
    },
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use bitcoin::{Network, OutPoint};
    use scraper::{Element, Selector};

    use crate::{
        btc_api::{
            esplora,
            ordinal::{fetch_outputs, Client, Ordinal, Output},
        },
        Print,
    };

    #[test]
    fn test_json() {
        let str = r#"{"address":"bc1pyf5f0r5eqxer5rdrwm98grgz5tem6k8xgtnm49he2m4kjhacrsms6p6888","indexed":true,"inscriptions":[],"runes":[["DOG•GO•TO•THE•MOON",{"amount":10000000000,"divisibility":5,"symbol":"🐕"}]],"sat_ranges":null,"script_pubkey":"OP_PUSHNUM_1 OP_PUSHBYTES_32 2268978e9901b23a0da376ca740d02a2f3bd58e642e7ba96f956eb695fb81c37","spent":false,"transaction":"24d006b4352792750fe2e7294cf9829db4e06cb11d1b4c5f03f9243c5622bc5f","value":546}"#;

        let a: Output = serde_json::from_str(str).unwrap();
        a.print();
    }

    #[test]
    fn test_client() {
        let c = Client::new("https://javirbin.com");
        c.fetch_output(
            &"24d006b4352792750fe2e7294cf9829db4e06cb11d1b4c5f03f9243c5622bc5f:3"
                .parse()
                .unwrap(),
        )
        .unwrap()
        .display()
        .print();
    }

    #[test]
    fn test() {
        let html = r#"<html lang="en"><head>
    <meta charset="utf-8">
    <meta name="format-detection" content="telephone=no">
    <meta name="viewport" content="width=device-width,initial-scale=1.0">
    <meta property="og:title" content="Output febf557cad7de5e17bddaa005fe584b1017cdaeca484111c33e5708b2c845951:2">
    <meta property="og:image" content="https://charlie.ordinals.net/static/favicon.png">
    <meta property="twitter:card" content="summary">
    <title>Output febf557cad7de5e17bddaa005fe584b1017cdaeca484111c33e5708b2c845951:2</title>
    <link rel="alternate" href="/feed.xml" type="application/rss+xml" title="Inscription Feed">
    <link rel="icon" href="/static/favicon.png">
    <link rel="icon" href="/static/favicon.svg">
    <link rel="stylesheet" href="/static/index.css">
    <link rel="stylesheet" href="/static/modern-normalize.css">
    <script src="/static/index.js" defer=""></script>
  </head>
  <body>
  <header>
    <nav>
      <a href="/" title="home">Ordinals<sup>alpha</sup></a>
      <a href="/inscriptions" title="inscriptions"><img class="icon" src="/static/images.svg"></a>
      <a href="/runes" title="runes"><img class="icon" src="/static/rune.svg"></a>
      <a href="/collections" title="collections"><img class="icon" src="/static/diagram-project.svg"></a>
      <a href="/blocks" title="blocks"><img class="icon" src="/static/cubes.svg"></a>
      <a href="/clock" title="clock"><img class="icon" src="/static/clock.svg"></a>
      <a href="/rare.txt" title="rare"><img class="icon" src="/static/gem.svg"></a>
      <a href="https://docs.ordinals.com/" title="handbook"><img class="icon" src="/static/book.svg"></a>
      <a href="https://github.com/ordinals/ord" title="github"><img class="icon" src="/static/github.svg"></a>
      <a href="https://discord.com/invite/ordinals" title="discord"><img class="icon" src="/static/discord.svg"></a>
      <form action="/search" method="get">
        <input type="text" autocapitalize="none" autocomplete="off" autocorrect="off" name="query" spellcheck="false">
        <input class="icon" type="image" src="/static/magnifying-glass.svg" alt="Search">
      </form>
    </nav>
  </header>
  <main>
<h1>Output <span class="monospace">febf557cad7de5e17bddaa005fe584b1017cdaeca484111c33e5708b2c845951:2</span></h1>
<dl>
  <dt>runes</dt>
  <dd>
    <table>
      <tbody><tr>
        <th>rune</th>
        <th>balance</th>
      </tr>
      <tr>
        <td><a href="/rune/DOG•GO•TO•THE•MOON">DOG•GO•TO•THE•MOON</a></td>
        <td>278064&nbsp;🐕</td>
      </tr>
    </tbody></table>
  </dd>
  <dt>value</dt><dd>546</dd>
  <dt>script pubkey</dt><dd class="monospace">OP_PUSHNUM_1 OP_PUSHBYTES_32 8db36bd2e7c75cbf3f8403eda14d5f61fdeac00c4a5e2179ab5a2a734ef5bc48</dd>
  <dt>address</dt><dd class="monospace">bc1p3kekh5h8cawt70uyq0k6zn2lv8774sqvff0zz7dttg48xnh4h3yqnuxhfj</dd>
  <dt>transaction</dt><dd><a class="monospace" href="/tx/febf557cad7de5e17bddaa005fe584b1017cdaeca484111c33e5708b2c845951">febf557cad7de5e17bddaa005fe584b1017cdaeca484111c33e5708b2c845951</a></dd>
  <dt>spent</dt><dd>true</dd>
</dl>
<h2>1 Sat Range</h2>
<ul class="monospace">
  <li><a href="/range/1028772346224832/1028772346225378" class="common">1028772346224832–1028772346225378</a></li>
</ul>

  </main>


</body></html>"#;
        let html = scraper::Html::parse_document(&html);
        let select_title = Selector::parse("main dl dt").unwrap();
        let title_select = html.select(&select_title).next().unwrap();
        let title = title_select.text().collect::<String>();

        let ordi = match title.as_str() {
            "inscriptions" => {
                let content = title_select.next_sibling_element().unwrap();
                let href_select = Selector::parse("a").unwrap();
                let href = content.select(&href_select).next().unwrap();
                // 铭文id
                let id = href
                    .attr("href")
                    .unwrap()
                    .trim_start_matches("/inscription/")
                    .to_string();
                // 读取铭文价值
                let value = content
                    .next_sibling_element()
                    .ok_or(anyhow!("No value title"))
                    .unwrap()
                    .next_sibling_element();
                Ordinal::Inscription {
                    id,
                    value: Default::default(),
                    out_point: Default::default(),
                }
            }
            "runes" => {
                let content = title_select.next_sibling_element().unwrap();
                let td_select = Selector::parse("td").unwrap();
                let tds = content.select(&td_select).collect::<Vec<_>>();
                let name = tds[0].text().collect();
                let number = tds[1].text().collect::<String>();
                println!("{}", name);
                println!("{}", number);

                Ordinal::Rune {
                    id: Default::default(),
                    name: name,
                    value: Default::default(),
                    number: number.parse().unwrap(),
                    div: 0,
                    out_point: Default::default(),
                }
            }
            _ => Ordinal::None,
        };
    }

    #[test]
    fn test_rune_id() {
        let html = r#"<html lang="en"><head>
    <meta charset="utf-8">
    <meta name="format-detection" content="telephone=no">
    <meta name="viewport" content="width=device-width,initial-scale=1.0">
    <meta property="og:title" content="Rune DOG•GO•TO•THE•MOON">
    <meta property="og:image" content="https://alpha.ordinals.net/static/favicon.png">
    <meta property="twitter:card" content="summary">
    <title>Rune DOG•GO•TO•THE•MOON</title>
    <link rel="alternate" href="/feed.xml" type="application/rss+xml" title="Inscription Feed">
    <link rel="icon" href="/static/favicon.png">
    <link rel="icon" href="/static/favicon.svg">
    <link rel="stylesheet" href="/static/index.css">
    <link rel="stylesheet" href="/static/modern-normalize.css">
    <script src="/static/index.js" defer=""></script>
  </head>
  <body>
  <header>
    <nav>
      <a href="/" title="home">Ordinals<sup>alpha</sup></a>
      <a href="/inscriptions" title="inscriptions"><img class="icon" src="/static/images.svg"></a>
      <a href="/runes" title="runes"><img class="icon" src="/static/rune.svg"></a>
      <a href="/collections" title="collections"><img class="icon" src="/static/diagram-project.svg"></a>
      <a href="/blocks" title="blocks"><img class="icon" src="/static/cubes.svg"></a>
      <a href="/clock" title="clock"><img class="icon" src="/static/clock.svg"></a>
      <a href="/rare.txt" title="rare"><img class="icon" src="/static/gem.svg"></a>
      <a href="https://docs.ordinals.com/" title="handbook"><img class="icon" src="/static/book.svg"></a>
      <a href="https://github.com/ordinals/ord" title="github"><img class="icon" src="/static/github.svg"></a>
      <a href="https://discord.com/invite/ordinals" title="discord"><img class="icon" src="/static/discord.svg"></a>
      <form action="/search" method="get">
        <input type="text" autocapitalize="none" autocomplete="off" autocorrect="off" name="query" spellcheck="false">
        <input class="icon" type="image" src="/static/magnifying-glass.svg" alt="Search">
      </form>
    </nav>
  </header>
  <main>
<h1>DOG•GO•TO•THE•MOON</h1>
  <div class="thumbnails">
    <a href="/inscription/e79134080a83fe3e0e06ed6990c5a9b63b362313341745707a2bff7d788a1375i0"><iframe sandbox="allow-scripts" scrolling="no" loading="lazy" src="/preview/e79134080a83fe3e0e06ed6990c5a9b63b362313341745707a2bff7d788a1375i0"></iframe></a>
  </div>
<dl>
  <dt>number</dt>
  <dd>3</dd>
  <dt>timestamp</dt>
  <dd><time title="Sat Apr 20 2024 08:09:27 GMT+0800 (中国标准时间)">2024-04-20 00:09:27 UTC</time></dd>
  <dt>id</dt>
  <dd>840000:3</dd>
  <dt>etching block</dt>
  <dd><a href="/block/840000">840000</a></dd>
  <dt>etching transaction</dt>
  <dd>3</dd>
  <dt>mint</dt>
  <dd>no</dd>
  <dt>supply</dt>
  <dd>100000000000&nbsp;🐕</dd>
  <dt>premine</dt>
  <dd>100000000000&nbsp;🐕</dd>
  <dt>premine percentage</dt>
  <dd>100%</dd>
  <dt>burned</dt>
  <dd>444903&nbsp;🐕</dd>
  <dt>divisibility</dt>
  <dd>5</dd>
  <dt>symbol</dt>
  <dd>🐕</dd>
  <dt>turbo</dt>
  <dd>false</dd>
  <dt>etching</dt>
  <dd><a class="monospace" href="/tx/e79134080a83fe3e0e06ed6990c5a9b63b362313341745707a2bff7d788a1375">e79134080a83fe3e0e06ed6990c5a9b63b362313341745707a2bff7d788a1375</a></dd>
  <dt>parent</dt>
  <dd><a class="monospace" href="/inscription/e79134080a83fe3e0e06ed6990c5a9b63b362313341745707a2bff7d788a1375i0">e79134080a83fe3e0e06ed6990c5a9b63b362313341745707a2bff7d788a1375i0</a></dd>
</dl>

  </main>
  

</body></html>"#;
        let html = scraper::Html::parse_document(&html);
        let select_title = Selector::parse("main dl dt").unwrap();
        let title_select = html.select(&select_title);

        let mut title_selected = None;
        for title_e in title_select {
            let title = title_e.text().collect::<String>();
            if title == "id" {
                title_selected = Some(title_e);
            }
        }
        let Some(title_selected) = title_selected else {
            println!("Not found rune id");
            return;
        };
        let id = title_selected
            .next_sibling_element()
            .unwrap()
            .text()
            .collect::<String>();
        println!("{}", id);
    }

    #[test]
    fn test_fetch_outs() {
        let c = esplora::new(Network::Bitcoin);
        let tx = c
            .get_transaction("8e03894c01e2ee33321756b6dee1b62d1803d512be04cac35ae30b038034a112")
            .unwrap();

        let u = tx
            .vin
            .iter()
            .map(|e| (e.txid.to_string(), e.vout as u64))
            .collect::<Vec<_>>();

        let all = fetch_outputs(vec![]).unwrap();

        println!("{:?}", all);
    }
}
