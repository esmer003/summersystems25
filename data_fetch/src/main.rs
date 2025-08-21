use serde::Deserialize;
use std::{fs::OpenOptions, io::Write, thread, time::Duration};

trait Pricing {
    fn fetch_price(&self) -> Option<f64>;
    fn save_to_file(&self, price: f64);
}

#[derive(Debug)]
struct Bitcoin;

#[derive(Debug)]
struct Ethereum;

#[derive(Debug)]
struct SP500;

// For CoinGecko (Bitcoin, Ethereum)
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

// For Yahoo Finance (S&P 500)
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

#[derive(Deserialize, Debug)]
struct Meta {
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: f64,
}

// ------------------ IMPLEMENTATIONS ------------------

impl Pricing for Bitcoin {
    fn fetch_price(&self) -> Option<f64> {
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
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open("sp500_prices.txt")
            .expect("Unable to open file");
        writeln!(file, "{}", price).unwrap();
    }
}

// ------------------ MAIN ------------------

fn main() {
    let assets: Vec<Box<dyn Pricing>> = vec![
        Box::new(Bitcoin),
        Box::new(Ethereum),
        Box::new(SP500),
    ];

    loop {
        for asset in &assets {
            if let Some(price) = asset.fetch_price() {
                println!("Fetched price: {}", price);
                asset.save_to_file(price);
            } else {
                eprintln!("Failed to fetch price");
            }

            // Add delay between requests to avoid rate limits
            thread::sleep(Duration::from_secs(3));
        }

        // Pause before starting the next full cycle
        println!("Waiting 10 seconds before next round...\n");
        thread::sleep(Duration::from_secs(10));
    }
}
