use anyhow::Result;
use clap::Parser;
use colored::*;
use numfmt::*;
use ratio_gang_cli::{
    get_required_envs, return_cyrpto_market_cap, return_gold_market_cap, return_stock_market_cap,
    ApiKeys, Error,
};
use reqwest::Client;
use serde_json::json;
use std::process;
const BAR_LENGTH: usize = 40;


#[derive(Parser)]
#[command(version
    , about=None, long_about = "Compare market caps between crypto, stock and gold by calculating their ratio\n- CLI returns percentages and market caps\n- Requires https://polygon.io and https://coingecko.com API Keys as environmental variables: POLYGON_KEY and COINGECKO_KEY")
    ]
struct Cli {
    #[arg(default_value = "ethereum")]
    asset_a: Option<String>,
    #[arg(default_value = "bitcoin")]
    asset_b: Option<String>,
    #[arg(
        long = "above-ground",
        default_value_t = 212582.0,
        help = "Set the estimated above ground stock of gold in tonnes"
    )]
    above_ground: f64,
    #[arg(
        short,
        long,
        help = "Return 'denumerator-asset numerator-asset percentage'. E.g.  'AAPL Gold 17'"
    )]
    plain: bool,
    #[arg(short, long, help = "Return json")]
    json: bool,
}

#[derive(Debug, PartialEq)]
enum MarketCapType {
    Gold,
    Stock,
    Crypto,
    Unknown,
}

fn identify_market_cap_type(market_cap: &String) -> MarketCapType {
    match market_cap.as_str() {
        "gold" | "Gold" => MarketCapType::Gold,
        s if s == s.to_uppercase() => MarketCapType::Stock,
        s if s == s.to_lowercase() => MarketCapType::Crypto,
        _ => MarketCapType::Unknown,
    }
}

async fn return_market_cap(
    client: &Client,
    asset_name: &String,
    apikeys: &ApiKeys,
    above_ground: &f64,
) -> Result<f64> {
    match identify_market_cap_type(asset_name) {
        MarketCapType::Gold => return_gold_market_cap(client, above_ground, apikeys).await,
        MarketCapType::Stock => return_stock_market_cap(client, asset_name, apikeys).await,
        MarketCapType::Crypto => return_cyrpto_market_cap(client, asset_name, apikeys).await,
        MarketCapType::Unknown => Err(Error::UnknownAssetName(asset_name.clone()).into()),
    }
}

fn create_ratio_gauge(ratio: f64, total_length: usize) -> String {
    if ratio < 0.0 || ratio > 1.0 {
        panic!("Ratio must be between 0 and 1");
    }
    let filled_length = (ratio * total_length as f64).round() as usize;
    let empty_length = total_length - filled_length;
    let filled_part = "█".repeat(filled_length).green(); // Green for filled part
    let empty_part = " ".repeat(empty_length); // Space for empty part
    let percentage = (ratio * 100.0).round() as usize;
    format!("[{}{}] {}%", filled_part, empty_part, percentage)
}

#[tokio::main]
async fn main() {
    let mut f = Formatter::default()
        .scales(Scales::short())
        .precision(Precision::Decimals(1));
    let client = reqwest::Client::new();
    let cli = Cli::parse();
    let apikeys = match get_required_envs() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{error}");
            process::exit(1)
        }
    };
    let above_ground = cli.above_ground;
    let asset_a = match cli.asset_a.as_ref() {
        Some(asset_a) => asset_a,
        None => {
            eprintln!("Missing left hand asset, see --help for usage");
            process::exit(1)
        }
    };
    let asset_b = match cli.asset_b.as_ref() {
        Some(asset_b) => asset_b,
        None => {
            eprintln!("Missing right hand asset, see --help for usage");
            process::exit(1)
        }
    };
    let left_hand_market_cap =
        match return_market_cap(&client, &asset_a, &apikeys, &above_ground).await {
            Ok(market_cap) => market_cap,
            Err(error) => {
                eprint!("{error}");
                process::exit(1)
            }
        };
    let right_hand_market_cap =
        match return_market_cap(&client, &asset_b, &apikeys, &above_ground).await {
            Ok(market_cap) => market_cap,
            Err(error) => {
                eprint!("{error}");
                process::exit(1)
            }
        };

    let (ratio, numerator_asset, denominator_asset, numerator_value, denominator_value) =
        if left_hand_market_cap < right_hand_market_cap {
            (
                left_hand_market_cap / right_hand_market_cap,
                asset_a,
                asset_b,
                left_hand_market_cap,
                right_hand_market_cap,
            )
        } else {
            (
                right_hand_market_cap / left_hand_market_cap,
                asset_b,
                asset_a,
                right_hand_market_cap,
                left_hand_market_cap,
            )
        };
    let percentage: u32 = (ratio * 100.0) as u32;
    if cli.plain {
        println!("{} {} {}", numerator_asset, denominator_asset, percentage)
    } else if cli.json {
        let json = json!({
            "percentage": percentage,
            "numerator": {
                "asset": numerator_asset,
                "market_cap": numerator_value as u64
            },
            "denominator": {
                "asset": denominator_asset,
                "market_cap": denominator_value as u64
            },
        });
        println!("{}", json.to_string());
    } else {
        println!("{}", create_ratio_gauge(ratio, BAR_LENGTH));
        println!("{}: {}", numerator_asset, f.fmt2(numerator_value));
        println!("{}: {}", denominator_asset, f.fmt2(denominator_value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_market_cap_type_for_gold() {
        let m = String::from("gold");
        assert_eq!(MarketCapType::Gold, identify_market_cap_type(&m))
    }
    #[test]
    fn test_identify_market_cap_type_for_capital_gold() {
        let m = String::from("Gold");
        assert_eq!(MarketCapType::Gold, identify_market_cap_type(&m))
    }
    #[test]
    fn test_identify_market_cap_type_for_crypto() {
        let m = String::from("ethereum");
        assert_eq!(MarketCapType::Crypto, identify_market_cap_type(&m))
    }
    #[test]
    fn test_identify_market_cap_type_for_stock() {
        let m = String::from("AAPL");
        assert_eq!(MarketCapType::Stock, identify_market_cap_type(&m))
    }
    #[test]
    fn test_identify_market_cap_type_for_unknown() {
        let m = String::from("FooBar");
        assert_eq!(MarketCapType::Unknown, identify_market_cap_type(&m))
    }
}
