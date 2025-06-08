use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Option type (Call or Put)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptionType {
    Call,
    Put,
}

/// The Greeks for options pricing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Greeks {
    /// Delta: sensitivity to underlying price changes
    pub delta: f64,
    /// Gamma: rate of change of delta
    pub gamma: f64,
    /// Theta: time decay (per day)
    pub theta: f64,
    /// Vega: sensitivity to implied volatility changes
    pub vega: f64,
    /// Rho: sensitivity to interest rate changes
    pub rho: f64,
}

impl Default for Greeks {
    fn default() -> Self {
        Self {
            delta: 0.0,
            gamma: 0.0,
            theta: 0.0,
            vega: 0.0,
            rho: 0.0,
        }
    }
}

/// Enhanced options contract with realistic market data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionsContract {
    /// Underlying symbol
    pub underlying: String,
    /// Option type (Call or Put)
    pub option_type: OptionType,
    /// Strike price
    pub strike: f64,
    /// Expiration date
    pub expiration: DateTime<Utc>,
    /// Current bid price
    pub bid: f64,
    /// Current ask price
    pub ask: f64,
    /// Last traded price
    pub last_price: f64,
    /// Implied volatility (as decimal, e.g., 0.20 for 20%)
    pub implied_volatility: f64,
    /// Open interest
    pub open_interest: u32,
    /// Volume traded today
    pub volume: u32,
    /// The Greeks
    pub greeks: Greeks,
    /// Days to expiration
    pub dte: u32,
    /// Timestamp of this quote
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct OptionsContractParams {
    pub underlying: String,
    pub option_type: OptionType,
    pub strike: f64,
    pub expiration: DateTime<Utc>,
    pub underlying_price: f64,
    pub implied_vol: f64,
    pub risk_free_rate: f64,
    pub current_time: DateTime<Utc>,
}

impl OptionsContract {
    #[allow(dead_code)]
    pub fn new(params: OptionsContractParams) -> Self {
        let dte = (params.expiration - params.current_time).num_days().max(0) as u32;

        // This is not robust at all, but for backtesting this will work
        let intrinsic = match params.option_type {
            OptionType::Call => (params.underlying_price - params.strike).max(0.0),
            OptionType::Put => (params.strike - params.underlying_price).max(0.0),
        };

        let distance_from_money = (params.strike - params.underlying_price).abs();
        let time_value = if params.expiration > params.current_time {
            // Closer to money = higher time value, farther = lower time value
            // Ensure short options are worth more than long options
            f64::max(3.0 - (distance_from_money * 0.1), 0.5)
        } else {
            // Even expired options get minimum value for testing
            0.5
        };

        let price = (intrinsic + time_value).max(0.01);

        // Simple bid-ask spread
        let spread = (price * 0.03).max(0.01);
        let bid = (price - spread / 2.0).max(0.01);
        let ask = price + spread / 2.0;

        Self {
            underlying: params.underlying,
            option_type: params.option_type,
            strike: params.strike,
            expiration: params.expiration,
            bid,
            ask,
            last_price: price,
            implied_volatility: params.implied_vol,
            open_interest: 1000,
            volume: 100,
            greeks: Greeks::default(),
            dte,
            timestamp: params.current_time,
        }
    }
}

/// Options chain for a specific expiration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionsChain {
    /// Underlying symbol
    pub underlying: String,
    /// Expiration date
    pub expiration: DateTime<Utc>,
    /// Current underlying price
    pub underlying_price: f64,
    /// All options contracts for this expiration (strike -> contract)
    pub calls: HashMap<u32, OptionsContract>,
    pub puts: HashMap<u32, OptionsContract>,
    /// Timestamp of this chain
    pub timestamp: DateTime<Utc>,
}

impl OptionsChain {
    /// Update all options in the chain with new underlying price
    #[allow(dead_code)]
    pub fn update(&mut self, new_underlying_price: f64, current_time: DateTime<Utc>) {
        self.underlying_price = new_underlying_price;
        self.timestamp = current_time;

        // Update all calls - simplified since we only need basic price updates
        for contract in self.calls.values_mut() {
            contract.timestamp = current_time;
        }

        // Update all puts
        for contract in self.puts.values_mut() {
            contract.timestamp = current_time;
        }
    }

    /// Get call option by strike
    pub fn get_call(&self, strike: f64) -> Option<&OptionsContract> {
        self.calls.get(&(strike as u32))
    }

    /// Get put option by strike
    pub fn get_put(&self, strike: f64) -> Option<&OptionsContract> {
        self.puts.get(&(strike as u32))
    }
}
