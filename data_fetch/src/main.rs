//imports
use serde::Deserialize;
use std::{fs::OpenOptions, io::Write, thread, time::Duration};

//defined price
trait Pricing {
    fn fetch_price(&self) -> Option<f64>;
    fn save_to_file(&self, price: f64);
}

//define structs
#[derive(Debug)]
struct Bitcoin;

#[derive(Debug)]
struct Ethereum;

#[derive(Debug)]
struct SP500;

//structs for apis
#[derive(Deserialize, Debug)]
struct CoinData {
    usd: f64,
}

#[derive(Deserialize, Debug)]
struct BitcoinResponse {
    bitcoin: CoinData,
}

#[derive(Deserialize, Debug)]
struct EthereumResponse {
    ethereum: CoinData,
}

//yahoo api
#[derive(Deserialize, Debug)]
struct YahooResponse {
    chart: Chart,
}

#[derive(Deserialize, Debug)]
struct Chart {
    result: Vec<ResultData>,
}

#[derive(Deserialize, Debug)]
struct ResultData {
    meta: Meta,
}

//matching api response
#[derive(Deserialize, Debug)]
struct Meta {
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: f64,
}

//implementations for assets
impl Pricing for Bitcoin {
    fn fetch_price(&self) -> Option<f64> {
        //bitcoin price
        let url = "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd";
        match ureq::get(url).call() {
            Ok(resp) => match resp.into_json::<BitcoinResponse>() {
                Ok(parsed) => Some(parsed.bitcoin.usd),
                Err(err) => {
                    eprintln!("Bitcoin JSON error: {}", err);
                    None
                }
            },
            Err(err) => {
                eprintln!("Bitcoin HTTP error: {}", err);
                None
            }
        }
    }

    fn save_to_file(&self, price: f64) {
        //writing price to file
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open("bitcoin_prices.txt")
            .expect("Unable to open file");
        writeln!(file, "{}", price).unwrap();
    }
}

impl Pricing for Ethereum {
    fn fetch_price(&self) -> Option<f64> {
        //ethereum price
        let url = "https://api.coingecko.com/api/v3/simple/price?ids=ethereum&vs_currencies=usd";
        match ureq::get(url).call() {
            Ok(resp) => match resp.into_json::<EthereumResponse>() {
                Ok(parsed) => Some(parsed.ethereum.usd),
                Err(err) => {
                    eprintln!("Ethereum JSON error: {}", err);
                    None
                }
            },
            Err(err) => {
                eprintln!("Ethereum HTTP error: {}", err);
                None
            }
        }
    }

    fn save_to_file(&self, price: f64) {
        //write price to file
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open("ethereum_prices.txt")
            .expect("Unable to open file");
        writeln!(file, "{}", price).unwrap();
    }
}

impl Pricing for SP500 {
    fn fetch_price(&self) -> Option<f64> {
        //get s&p 500 index price
        let url = "https://query2.finance.yahoo.com/v8/finance/chart/%5EGSPC";
        match ureq::get(url).call() {
            Ok(resp) => match resp.into_json::<YahooResponse>() {
                Ok(parsed) => Some(parsed.chart.result[0].meta.regular_market_price),
                Err(err) => {
                    eprintln!("SP500 JSON error: {}", err);
                    None
                }
            },
            Err(err) => {
                eprintln!("SP500 HTTP error: {}", err);
                None
            }
        }
    }

    fn save_to_file(&self, price: f64) {
        //write price to file
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open("sp500_prices.txt")
            .expect("Unable to open file");
        writeln!(file, "{}", price).unwrap();
    }
}

//program
fn main() {
    //lists of assets
    let assets: Vec<Box<dyn Pricing>> = vec![
        Box::new(Bitcoin),
        Box::new(Ethereum),
        Box::new(SP500),
    ];

    //repeat
    loop {
        for asset in &assets {
            //fetch and print price
            if let Some(price) = asset.fetch_price() {
                println!("Fetched price: {}", price);
                asset.save_to_file(price);
            } else {
                eprintln!("Failed to fetch price");
            }
            //pause 3 secs btw requests
            thread::sleep(Duration::from_secs(3));
        }
        //wait before next round
        println!("Waiting 10 seconds before next round...\n");
        thread::sleep(Duration::from_secs(10));
    }
}
