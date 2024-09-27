use anyhow::Result;
use reqwest::header::USER_AGENT;
use serde::{Deserialize, Serialize};
use std::env;
use std::str::FromStr;
use url::{ParseError, Url};

const POLYGONIO_BASE_URL: &str = "https://api.polygon.io";
const COINGECKO_BASE_URL: &str = "https://api.coingecko.com";
const TONNE_TO_OUNCE: f64 = 35273.96194958;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("URL is not valid: {0}\n")]
    InvalidUrl(ParseError),
    #[error("Error sending request: {0}\n")]
    SendRequest(reqwest::Error),
    #[error("Failed to deserialize response: '{0}' for asset {1}\n")]
    Deserialization(serde_json::Error, String),
    #[error("Unexpected status code: {0}\n")]
    UnexpectedStatus(reqwest::Error),
    #[error("Required environmental variable not set. Use 'export {0}=YOURKEY' to set it.\n")]
    EnvVarError(String),
    #[error("Polygon API error: {0}\n")]
    PolygonApi(String),
    #[error("Coingecko API did not return expected payload.\nReceived {0}, expected https://docs.coingecko.com/reference/coins-markets\n")]
    CoingeckoApi(String),
    #[error("Could not identify if {0} is a crypto asset or a stock, please use all caps for stock symbols and lower caps for crypto coingecko-ids\n")]
    UnknownAssetName(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CoingeckoMarketsV2 {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub image: String,
    pub current_price: f64,
    pub market_cap: f64,
    pub market_cap_rank: u32,
    pub fully_diluted_valuation: u64,
    pub total_volume: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub price_change_24h: f64,
    pub price_change_percentage_24h: f64,
    pub market_cap_change_24h: f64,
    pub market_cap_change_percentage_24h: f64,
    pub circulating_supply: f64,
    pub total_supply: f64,
    pub max_supply: Option<f64>,
    pub ath: f64,
    pub ath_change_percentage: f64,
    pub ath_date: String,
    pub atl: f64,
    pub atl_change_percentage: f64,
    pub atl_date: String,
    pub roi: Option<Roi>,
    pub last_updated: String,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Roi {
    pub times: f64,
    pub currency: String,
    pub percentage: f64,
}

#[derive(Serialize, Deserialize, Debug)]
struct Address {
    address1: String,
    city: String,
    postal_code: String,
    state: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Branding {
    icon_url: String,
    logo_url: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct CompanyDetails {
    active: bool,
    address: Address,
    branding: Option<Branding>,
    cik: String,
    composite_figi: String,
    currency_name: String,
    description: Option<String>,
    homepage_url: Option<String>,
    list_date: Option<String>,
    locale: String,
    market: String,
    market_cap: f64,
    name: String,
    phone_number: String,
    primary_exchange: String,
    round_lot: u32,
    share_class_figi: String,
    share_class_shares_outstanding: u64,
    sic_code: String,
    sic_description: String,
    ticker: String,
    ticker_root: String,
    total_employees: u32,
    #[serde(rename(serialize = "type", deserialize = "type"))]
    type_: String, // Using type_ because 'type' is a reserved keyword
    weighted_shares_outstanding: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct TickerDetailsV3 {
    status: String,
    request_id: String,
    results: CompanyDetails,
}

#[derive(Serialize, Deserialize, Debug)]
struct AggsTickerV2 {
    ticker: String,
    #[serde(rename(serialize = "queryCount", deserialize = "queryCount"))]
    query_count: u32,
    #[serde(rename(serialize = "resultsCount", deserialize = "resultsCount"))]
    results_count: u32,
    adjusted: bool,
    results: Vec<OHCL>,
    status: String,
    request_id: String,
    count: u32,
}

/// Previous day's open, high, low, and close (OHCL)
#[derive(Serialize, Deserialize, Debug)]
struct OHCL {
    #[serde(rename(serialize = "T", deserialize = "T"))]
    ticker: String,
    v: u32,
    vw: f64,
    o: f64,
    c: f64,
    h: f64,
    l: f64,
    #[serde(rename(serialize = "t", deserialize = "t"))]
    timestamp: u64,
    n: u32,
}

#[derive(Serialize, Deserialize, Debug)]
struct PolygonIoErrorResponse {
    status: String,
    request_id: String,
    message: String,
}

// holds api keys from system env
#[derive(Debug, Default)]
pub struct ApiKeys {
    coingecko: String,
    polygonio: String,
}

pub fn get_required_envs() -> Result<ApiKeys, Error> {
    let mut apikeys = ApiKeys::default();
    let polygon_env = String::from("POLYGON_KEY");
    let coingecko_env = String::from("COINGECKO_KEY");

    match env::var(&polygon_env) {
        Ok(value) => apikeys.polygonio = value,
        Err(_) => {
            return Err(Error::EnvVarError(polygon_env));
        }
    }
    match env::var(&coingecko_env) {
        Ok(value) => apikeys.coingecko = value,
        Err(_) => {
            return Err(Error::EnvVarError(coingecko_env));
        }
    }
    Ok(apikeys)
}

fn construct_coingecko_v3_markets_query_url(
    coingecko_id: &String,
    apikey: &String,
) -> Result<Url, Error> {
    match Url::from_str(&format!("{COINGECKO_BASE_URL}/api/v3/coins/markets")) {
        Ok(mut url) => {
            url.query_pairs_mut()
                .append_pair("vs_currency", "usd")
                .append_pair("ids", &coingecko_id)
                .append_pair("x_cg_key", &apikey);
            return Ok(url);
        }
        Err(error) => return Err(Error::InvalidUrl(error)),
    }
}

fn construct_tickerdetailsv3_query_url(
    stock_symbol: &String,
    apikey: &String,
) -> Result<Url, Error> {
    match Url::from_str(&format!(
        "{POLYGONIO_BASE_URL}/v3/reference/tickers/{stock_symbol}"
    )) {
        Ok(mut url) => {
            url.query_pairs_mut().append_pair("apiKey", &apikey);
            return Ok(url);
        }
        Err(error) => return Err(Error::InvalidUrl(error)),
    }
}

fn construct_forex_query_url(forex_ticker: &String, apikey: &String) -> Result<Url, Error> {
    match Url::from_str(&format!(
        "{POLYGONIO_BASE_URL}/v2/aggs/ticker/C:{forex_ticker}/prev"
    )) {
        Ok(mut url) => {
            url.query_pairs_mut().append_pair("apiKey", &apikey);
            return Ok(url);
        }
        Err(error) => return Err(Error::InvalidUrl(error)),
    }
}

pub async fn return_stock_market_cap(
    client: &reqwest::Client,
    stock_symbol: &String,
    apikeys: &ApiKeys,
) -> Result<f64, anyhow::Error> {
    let url = construct_tickerdetailsv3_query_url(&stock_symbol, &apikeys.polygonio)?;
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(Error::SendRequest)?;

    if response.status().is_success() {
        let body = response.text().await?;
        let ticker_details_v3: TickerDetailsV3 = serde_json::from_str(&body)
            .map_err(|e| Error::Deserialization(e, stock_symbol.clone()))?;
        Ok(ticker_details_v3.results.market_cap)
    } else {
        let body = response.text().await?;
        let error_json: PolygonIoErrorResponse = serde_json::from_str(&body)
            .map_err(|e| Error::Deserialization(e, stock_symbol.clone()))?;
        return Err(Error::PolygonApi(error_json.message).into());
    }
}

pub async fn return_gold_market_cap(
    client: &reqwest::Client,
    above_ground: &f64,
    apikeys: &ApiKeys,
) -> Result<f64> {
    let gold_ticker = String::from("XAUUSD");
    let url = construct_forex_query_url(&gold_ticker, &apikeys.polygonio)?;
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(Error::SendRequest)?;

    if response.status().is_success() {
        let body = response.text().await?;
        let aggs_ticker_v2: AggsTickerV2 =
            serde_json::from_str(&body).map_err(|e| Error::Deserialization(e, gold_ticker))?;
        let gold_market_cap: f64 = aggs_ticker_v2.results[0].c * above_ground * TONNE_TO_OUNCE;
        Ok(gold_market_cap)
    } else {
        let body = response.text().await?;
        let error_json: PolygonIoErrorResponse =
            serde_json::from_str(&body).map_err(|e| Error::Deserialization(e, gold_ticker))?;
        return Err(Error::PolygonApi(error_json.message).into());
    }
}

pub async fn return_cyrpto_market_cap(
    client: &reqwest::Client,
    coingecko_id: &String,
    apikeys: &ApiKeys,
) -> Result<f64> {
    let url = construct_coingecko_v3_markets_query_url(&coingecko_id, &apikeys.coingecko)?;
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(Error::SendRequest)?;

    if response.status().is_success() {
        let body = response.text().await?;
        // []
        if body == "[]" {
            return Err(Error::CoingeckoApi(body).into());
        }
        let coingecko_markets_v3: Vec<CoingeckoMarketsV2> = serde_json::from_str(&body)
            .map_err(|e| Error::Deserialization(e, coingecko_id.clone()))?;
        Ok(coingecko_markets_v3[0].market_cap)
    } else {
        let body = response.text().await?;
        return Err(Error::CoingeckoApi(body).into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_construct_forex_query_url() {
        let apikeys = ApiKeys {
            coingecko: String::from("myCoinGeckoKey"),
            polygonio: String::from("myPolygonIOKey"),
        };
        let forex_ticker = &String::from("XAUUSD");
        let constructed_url = construct_forex_query_url(&forex_ticker, &apikeys.polygonio).unwrap();
        let target_url =
            Url::parse("https://api.polygon.io/v2/aggs/ticker/C:XAUUSD/prev?apiKey=myPolygonIOKey")
                .unwrap();
        assert_eq!(constructed_url, target_url);
    }

    #[test]
    fn test_construct_tickerdetailsv3_query_url() {
        let apikeys = ApiKeys {
            coingecko: String::from("myCoinGeckoKey"),
            polygonio: String::from("myPolygonIOKey"),
        };
        let stock_symbol = &String::from("AAPL");
        let constructed_url =
            construct_tickerdetailsv3_query_url(&stock_symbol, &apikeys.polygonio).unwrap();
        let target_url =
            Url::parse("https://api.polygon.io/v3/reference/tickers/AAPL?apiKey=myPolygonIOKey")
                .unwrap();
        assert_eq!(constructed_url, target_url);
    }

    #[test]
    fn test_construct_coingecko_v3_markets_query_url() {
        let apikeys = ApiKeys {
            coingecko: String::from("myCoinGeckoKey"),
            polygonio: String::from("myPolygonIOKey"),
        };
        let coingecko_id = &String::from("ethereum");
        let constructed_url =
            construct_coingecko_v3_markets_query_url(&coingecko_id, &apikeys.coingecko).unwrap();
        let target_url =
            Url::parse("https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&ids=ethereum&x_cg_key=myCoinGeckoKey")
                .unwrap();
        assert_eq!(constructed_url, target_url);
    }

    #[test]
    fn test_get_required_envs_returns_keys_if_set() {
        env::set_var("COINGECKO_KEY", "foo");
        env::set_var("POLYGON_KEY", "bar");
        let api_keys = get_required_envs().unwrap();
        assert_eq!(api_keys.coingecko, "foo");
        assert_eq!(api_keys.polygonio, "bar");
    }

    #[test]
    fn test_get_required_envs_returns_error_if_not_set() {
        env::remove_var("COINGECKO_KEY");
        env::remove_var("POLYGON_KEY");
        let result = get_required_envs();
        match result {
            Err(Error::EnvVarError(..)) => assert!(true),
            _ => assert!(false, "Expected Error::EnvVarError"),
        }
    }

    #[test]
    fn test_deserialize_ticker_details_v3() {
        let data = r#"
            {
              "request_id": "102a3351cebaf560a070c6002c3b1d91",
              "results": {
                "ticker": "AAPL",
                "name": "Apple Inc.",
                "market": "stocks",
                "locale": "us",
                "primary_exchange": "XNAS",
                "type": "CS",
                "active": true,
                "currency_name": "usd",
                "cik": "0000320193",
                "composite_figi": "BBG000B9XRY4",
                "share_class_figi": "BBG001S5N8V8",
                "market_cap": 3.38702559949E+12,
                "phone_number": "(408) 996-1010",
                "address": {
                  "address1": "ONE APPLE PARK WAY",
                  "city": "CUPERTINO",
                  "state": "CA",
                  "postal_code": "95014"
                },
                "description": "Apple is among the largest companies in the world, with a broad portfolio of hardware and software products targeted at consumers and businesses. Apple's iPhone makes up a majority of the firm sales, and Apple's other products like Mac, iPad, and Watch are designed around the iPhone as the focal point of an expansive software ecosystem. Apple has progressively worked to add new applications, like streaming video, subscription bundles, and augmented reality. The firm designs its own software and semiconductors while working with subcontractors like Foxconn and TSMC to build its products and chips. Slightly less than half of Apple's sales come directly through its flagship stores, with a majority of sales coming indirectly through partnerships and distribution.",
                "sic_code": "3571",
                "sic_description": "ELECTRONIC COMPUTERS",
                "ticker_root": "AAPL",
                "homepage_url": "https://www.apple.com",
                "total_employees": 161000,
                "list_date": "1980-12-12",
                "branding": {
                  "logo_url": "https://api.polygon.io/v1/reference/company-branding/YXBwbGUuY29t/images/2024-09-01_logo.svg",
                  "icon_url": "https://api.polygon.io/v1/reference/company-branding/YXBwbGUuY29t/images/2024-09-01_icon.png"
                },
                "share_class_shares_outstanding": 15204140000,
                "weighted_shares_outstanding": 15204137000,
                "round_lot": 100
              },
              "status": "OK"
            }
            "#;
        let result: Result<TickerDetailsV3, serde_json::Error> = serde_json::from_str(data);
        // check if result is ok
        assert!(result.is_ok());
        // check one value
        let ticker_details_v3 = result.unwrap();
        assert_eq!(ticker_details_v3.results.ticker, "AAPL");
    }

    #[test]
    fn test_deserialize_aggs_ticker_v2() {
        let data = r#"
            {
                "ticker": "C:XAUUSD",
                "queryCount": 1,
                "resultsCount": 1,
                "adjusted": true,
                "results": [
                    {
                        "T": "C:XAUUSD",
                        "v": 3560,
                        "vw": 2570.3368,
                        "o": 2574.07,
                        "c": 2559.15,
                        "h": 2599.8,
                        "l": 2547.63,
                        "t": 1726703999999,
                        "n": 3560
                    }
                ],
                "status": "OK",
                "request_id": "852639747d77390dc13e683c4938d3c8",
                "count": 1
            }
            "#;
        let aggs_ticker_v2: AggsTickerV2 = serde_json::from_str(data).unwrap();
        assert_eq!(aggs_ticker_v2.results[0].c, 2559.15);
    }

    #[test]
    fn test_deserialize_coingecko_markets_v3() {
        let data = r#"
            [
              {
                "id": "ethereum",
                "symbol": "eth",
                "name": "Ethereum",
                "image": "https://coin-images.coingecko.com/coins/images/279/large/ethereum.png?1696501628",
                "current_price": 2431.96,
                "market_cap": 292802217292,
                "market_cap_rank": 2,
                "fully_diluted_valuation": 292802217292,
                "total_volume": 20902271271,
                "high_24h": 2440.58,
                "low_24h": 2285.67,
                "price_change_24h": 110.02,
                "price_change_percentage_24h": 4.73821,
                "market_cap_change_24h": 13366145157,
                "market_cap_change_percentage_24h": 4.78326,
                "circulating_supply": 120345065.769204,
                "total_supply": 120345065.769204,
                "max_supply": null,
                "ath": 4878.26,
                "ath_change_percentage": -50.09723,
                "ath_date": "2021-11-10T14:24:19.604Z",
                "atl": 0.432979,
                "atl_change_percentage": 562141.58481,
                "atl_date": "2015-10-20T00:00:00.000Z",
                "roi": {
                  "times": 51.51623725311915,
                  "currency": "btc",
                  "percentage": 5151.623725311915
                },
                "last_updated": "2024-09-19T08:55:01.703Z"
              }
            ]
            "#;
        let result: Vec<CoingeckoMarketsV2> = serde_json::from_str(data).unwrap();
        let market_cap = result[0].market_cap;
        assert_eq!(market_cap, 292802217292.0)
    }
}
