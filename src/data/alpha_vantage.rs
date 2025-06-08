use chrono::{DateTime, Utc};
use reqwest;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use tracing::info;

use crate::backtest::runner::EnhancedMarketEvent;
use crate::models::options_data::{Greeks, OptionType, OptionsChain, OptionsContract};

/// Alpha Vantage API response structures
#[derive(Debug, Deserialize)]
struct AlphaVantageResponse {
    #[serde(rename = "data")]
    data: Option<Vec<AlphaVantageOption>>,
    #[serde(rename = "Error Message")]
    error_message: Option<String>,
    #[serde(rename = "Note")]
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct AlphaVantageOption {
    #[serde(rename = "contractID")]
    contract_id: String,
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "expiration")]
    expiration: String,
    #[serde(rename = "strike")]
    strike: String,
    #[serde(rename = "type")]
    option_type: String,
    #[serde(rename = "last")]
    last: Option<String>,
    #[serde(rename = "mark")]
    mark: Option<String>,
    #[serde(rename = "bid")]
    bid: Option<String>,
    #[serde(rename = "ask")]
    ask: Option<String>,
    #[serde(rename = "volume")]
    volume: Option<String>,
    #[serde(rename = "open_interest")]
    open_interest: Option<String>,
    #[serde(rename = "date")]
    date: Option<String>,
    #[serde(rename = "implied_volatility")]
    implied_volatility: Option<String>,
    #[serde(rename = "delta")]
    delta: Option<String>,
    #[serde(rename = "gamma")]
    gamma: Option<String>,
    #[serde(rename = "theta")]
    theta: Option<String>,
    #[serde(rename = "vega")]
    vega: Option<String>,
    #[serde(rename = "rho")]
    rho: Option<String>,
}

/// Alpha Vantage API client for fetching options data
pub struct AlphaVantageClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl AlphaVantageClient {
    /// Create a new Alpha Vantage client
    /// Get your free API key from: https://www.alphavantage.co/support/#api-key
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: "https://www.alphavantage.co".to_string(),
        }
    }

    /// Fetch current options chain for a symbol using HISTORICAL_OPTIONS endpoint
    /// This is free and includes recent data
    pub async fn fetch_options_chain(
        &self,
        symbol: &str,
    ) -> Result<EnhancedMarketEvent, Box<dyn Error>> {
        info!("Fetching options chain for {} from Alpha Vantage", symbol);

        let url = format!(
            "{}/query?function=HISTORICAL_OPTIONS&symbol={}&apikey={}",
            self.base_url, symbol, self.api_key
        );
        info!(url);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "options-trading-engine/1.0")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Alpha Vantage API error: {}", response.status()).into());
        }

        let alpha_data: AlphaVantageResponse = response.json().await?;

        if let Some(error) = alpha_data.error_message {
            return Err(format!("Alpha Vantage API error: {}", error).into());
        }

        if let Some(note) = alpha_data.note {
            if note.contains("API call frequency") {
                return Err("Alpha Vantage API rate limit exceeded".into());
            }
        }

        let options_data = alpha_data
            .data
            .ok_or("No options data returned from Alpha Vantage")?;

        if options_data.is_empty() {
            return Err("Empty options data from Alpha Vantage".into());
        }

        let event = self.convert_to_enhanced_market_event(&options_data, symbol)?;

        info!(
            "Successfully fetched {} options contracts for {} from Alpha Vantage",
            event
                .options_chains
                .values()
                .map(|c| c.calls.len() + c.puts.len())
                .sum::<usize>(),
            symbol
        );

        Ok(event)
    }

    /// Convert Alpha Vantage data to our internal EnhancedMarketEvent format
    fn convert_to_enhanced_market_event(
        &self,
        options_data: &[AlphaVantageOption],
        symbol: &str,
    ) -> Result<EnhancedMarketEvent, Box<dyn Error>> {
        let current_time = Utc::now();

        // Calculate a reasonable underlying price from the options data
        // Use the average of strike prices as a rough estimate
        let strikes: Vec<f64> = options_data
            .iter()
            .filter_map(|opt| opt.strike.parse::<f64>().ok())
            .collect();

        let underlying_price = if strikes.is_empty() {
            500.0 // TODO replace fallback
        } else {
            strikes.iter().sum::<f64>() / strikes.len() as f64
        };

        // Calculate average implied volatility
        let valid_ivs: Vec<f64> = options_data
            .iter()
            .filter_map(|opt| opt.implied_volatility.as_ref()?.parse::<f64>().ok())
            .collect();
        let avg_iv = if valid_ivs.is_empty() {
            0.20
        } else {
            valid_ivs.iter().sum::<f64>() / valid_ivs.len() as f64
        };

        // Calculate total volume
        let total_volume = options_data
            .iter()
            .filter_map(|opt| opt.volume.as_ref()?.parse::<f64>().ok())
            .sum::<f64>();

        // Group options by expiration
        let mut options_by_expiration: HashMap<String, Vec<&AlphaVantageOption>> = HashMap::new();

        for option in options_data {
            options_by_expiration
                .entry(option.expiration.clone())
                .or_default()
                .push(option);
        }

        // Create options chains
        let mut options_chains = HashMap::new();

        for (exp_key, exp_options) in options_by_expiration {
            // Parse expiration date
            let expiration = chrono::NaiveDate::parse_from_str(&exp_key, "%Y-%m-%d")
                .map_err(|_| "Invalid expiration date format")?
                .and_hms_opt(16, 0, 0)
                .ok_or("Invalid time")?
                .and_utc();

            let mut calls = HashMap::new();
            let mut puts = HashMap::new();

            for option in exp_options {
                let option_type = match option.option_type.to_lowercase().as_str() {
                    "call" => OptionType::Call,
                    "put" => OptionType::Put,
                    _ => continue,
                };

                let contract = self.convert_alpha_vantage_option(
                    option,
                    option_type,
                    symbol,
                    underlying_price,
                    expiration,
                    current_time,
                )?;

                let strike = option
                    .strike
                    .parse::<f64>()
                    .map_err(|_| "Invalid strike price")?;
                let strike_key = strike as u32;

                match option_type {
                    OptionType::Call => calls.insert(strike_key, contract),
                    OptionType::Put => puts.insert(strike_key, contract),
                };
            }

            if !calls.is_empty() || !puts.is_empty() {
                let chain = OptionsChain {
                    underlying: symbol.to_string(),
                    expiration,
                    underlying_price,
                    calls,
                    puts,
                    timestamp: current_time,
                };

                options_chains.insert(exp_key, chain);
            }
        }

        if options_chains.is_empty() {
            return Err("No valid options chains created from Alpha Vantage response".into());
        }

        Ok(EnhancedMarketEvent {
            symbol: symbol.to_string(),
            underlying_price,
            volume: total_volume,
            implied_volatility: avg_iv,
            options_chains,
            timestamp: current_time,
        })
    }

    /// Convert a single Alpha Vantage option to our internal format
    fn convert_alpha_vantage_option(
        &self,
        alpha_option: &AlphaVantageOption,
        option_type: OptionType,
        underlying: &str,
        _underlying_price: f64,
        expiration: DateTime<Utc>,
        current_time: DateTime<Utc>,
    ) -> Result<OptionsContract, Box<dyn Error>> {
        // Parse numeric fields safely
        let strike = alpha_option
            .strike
            .parse::<f64>()
            .map_err(|_| "Invalid strike price")?;

        let bid = alpha_option
            .bid
            .as_ref()
            .and_then(|b| b.parse::<f64>().ok())
            .unwrap_or(0.01);

        let ask = alpha_option
            .ask
            .as_ref()
            .and_then(|a| a.parse::<f64>().ok())
            .unwrap_or(bid + 0.01);

        let last_price = alpha_option
            .last
            .as_ref()
            .and_then(|l| l.parse::<f64>().ok())
            .or(alpha_option
                .mark
                .as_ref()
                .and_then(|m| m.parse::<f64>().ok()))
            .unwrap_or((bid + ask) / 2.0);

        let volume = alpha_option
            .volume
            .as_ref()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);

        let open_interest = alpha_option
            .open_interest
            .as_ref()
            .and_then(|oi| oi.parse::<u32>().ok())
            .unwrap_or(0);

        let implied_volatility = alpha_option
            .implied_volatility
            .as_ref()
            .and_then(|iv| iv.parse::<f64>().ok())
            .unwrap_or(0.20);

        // Parse Greeks if available
        let greeks = Greeks {
            delta: alpha_option
                .delta
                .as_ref()
                .and_then(|d| d.parse::<f64>().ok())
                .unwrap_or(0.0),
            gamma: alpha_option
                .gamma
                .as_ref()
                .and_then(|g| g.parse::<f64>().ok())
                .unwrap_or(0.0),
            theta: alpha_option
                .theta
                .as_ref()
                .and_then(|t| t.parse::<f64>().ok())
                .unwrap_or(0.0),
            vega: alpha_option
                .vega
                .as_ref()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0),
            rho: alpha_option
                .rho
                .as_ref()
                .and_then(|r| r.parse::<f64>().ok())
                .unwrap_or(0.0),
        };

        Ok(OptionsContract {
            underlying: underlying.to_string(),
            option_type,
            strike,
            expiration,
            bid,
            ask,
            last_price,
            implied_volatility,
            open_interest,
            volume,
            greeks,
            dte: (expiration - current_time).num_days().max(0) as u32,
            timestamp: current_time,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_alpha_vantage_demo() {
        let client = AlphaVantageClient::new("demo".to_string());

        match client.fetch_options_chain("SPY").await {
            Ok(event) => {
                println!("Symbol: {}", event.symbol);
                println!("Underlying Price: ${:.2}", event.underlying_price);
                println!("Options Chains: {}", event.options_chains.len());
                println!("Total Volume: {:.0}", event.volume);
                println!("Avg IV: {:.1}%", event.implied_volatility * 100.0);

                for (exp_date, chain) in event.options_chains.iter() {
                    println!(
                        "Expiration {}: {} calls, {} puts",
                        exp_date,
                        chain.calls.len(),
                        chain.puts.len()
                    );
                }

                assert!(!event.options_chains.is_empty());
                assert!(event.underlying_price > 0.0);
            }
            Err(e) => {
                println!("Alpha Vantage demo test info: {}", e);
            }
        }
    }
}
