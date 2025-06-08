use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::options_data::OptionsContract;

/// A complete iron condor position with real options contracts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IronCondorPosition {
    /// Unique position ID
    pub id: Uuid,
    /// Underlying symbol
    pub underlying: String,
    /// Entry timestamp
    pub entry_time: DateTime<Utc>,
    /// Exit timestamp (None if still open)
    pub exit_time: Option<DateTime<Utc>>,

    // The four legs of an iron condor
    /// Short call (higher strike)
    pub short_call: OptionsContract,
    /// Long call (even higher strike, protection)
    pub long_call: OptionsContract,
    /// Short put (lower strike)
    pub short_put: OptionsContract,
    /// Long put (even lower strike, protection)
    pub long_put: OptionsContract,

    /// Number of contracts (typically 1)
    pub quantity: u32,
    /// Entry premium received (net credit)
    pub entry_premium: f64,
    /// Exit premium paid (net debit, if closed early)
    pub exit_premium: Option<f64>,

    /// Reason for exit (if closed)
    pub exit_reason: Option<String>,
}

impl IronCondorPosition {
    pub fn new(
        underlying: String,
        short_call: OptionsContract,
        long_call: OptionsContract,
        short_put: OptionsContract,
        long_put: OptionsContract,
        quantity: u32,
        entry_time: DateTime<Utc>,
    ) -> Self {
        // Calculate net premium received (credit spread)
        let entry_premium =
            (short_call.bid + short_put.bid - long_call.ask - long_put.ask) * quantity as f64;

        Self {
            id: Uuid::new_v4(),
            underlying,
            entry_time,
            exit_time: None,
            short_call,
            long_call,
            short_put,
            long_put,
            quantity,
            entry_premium,
            exit_premium: None,
            exit_reason: None,
        }
    }

    /// Check if the position is still open
    #[allow(dead_code)]
    pub fn is_open(&self) -> bool {
        self.exit_time.is_none()
    }

    /// Get the width of the call spread
    pub fn call_spread_width(&self) -> f64 {
        self.long_call.strike - self.short_call.strike
    }

    /// Get the width of the put spread
    pub fn put_spread_width(&self) -> f64 {
        self.short_put.strike - self.long_put.strike
    }

    /// Get maximum profit (premium received)
    pub fn max_profit(&self) -> f64 {
        self.entry_premium
    }

    /// Get maximum loss (spread width minus premium received)
    pub fn max_loss(&self) -> f64 {
        let max_call_loss = self.call_spread_width() * self.quantity as f64;
        let max_put_loss = self.put_spread_width() * self.quantity as f64;
        // Use the wider spread for max loss calculation
        max_call_loss.max(max_put_loss) - self.entry_premium
    }

    /// Calculate current P&L based on current option prices
    pub fn calculate_pnl(&self, current_underlying_price: f64) -> f64 {
        if let Some(exit_premium) = self.exit_premium {
            // Position was closed - use actual exit premium
            self.entry_premium - exit_premium
        } else {
            // Position still open - calculate mark-to-market P&L
            self.calculate_unrealized_pnl(current_underlying_price)
        }
    }

    /// Calculate unrealized P&L (mark-to-market)
    fn calculate_unrealized_pnl(&self, current_underlying_price: f64) -> f64 {
        // TODO use current option prices with time value

        let call_spread_pnl = if current_underlying_price >= self.long_call.strike {
            // Max loss on call spread
            -(self.call_spread_width() * self.quantity as f64)
        } else if current_underlying_price <= self.short_call.strike {
            // Max profit on call spread
            0.0
        } else {
            // Partially in-the-money
            -((current_underlying_price - self.short_call.strike) * self.quantity as f64)
        };

        let put_spread_pnl = if current_underlying_price <= self.long_put.strike {
            // Max loss on put spread
            -(self.put_spread_width() * self.quantity as f64)
        } else if current_underlying_price >= self.short_put.strike {
            // Max profit on put spread
            0.0
        } else {
            // Partially in-the-money
            -((self.short_put.strike - current_underlying_price) * self.quantity as f64)
        };

        self.entry_premium + call_spread_pnl + put_spread_pnl
    }


    /// Get the profit percentage based on max profit
    pub fn profit_percentage(&self, current_underlying_price: f64) -> f64 {
        let current_pnl = self.calculate_pnl(current_underlying_price);
        if self.max_profit() > 0.0 {
            (current_pnl / self.max_profit()) * 100.0
        } else {
            0.0
        }
    }


    /// Get days to expiration (using short call expiration)
    pub fn days_to_expiration(&self, current_time: DateTime<Utc>) -> i64 {
        (self.short_call.expiration - current_time).num_days()
    }

    /// Get a summary string of the position
    pub fn summary(&self) -> String {
        format!(
            "IC {}: {}C/{}C {}P/{}P @{:.2} profit={:.2} max_loss={:.2}",
            self.underlying,
            self.short_call.strike,
            self.long_call.strike,
            self.short_put.strike,
            self.long_put.strike,
            self.entry_premium,
            self.max_profit(),
            self.max_loss()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::options_data::{OptionType, OptionsContract, OptionsContractParams};

    #[test]
    fn test_iron_condor_creation() {
        let now = Utc::now();
        let expiration = now + chrono::Duration::days(1);

        let short_call = OptionsContract::new(
            OptionsContractParams {
                underlying: "SPY".to_string(),
                option_type: OptionType::Call,
                strike: 410.0,
                expiration,
                underlying_price: 400.0, // underlying price
                implied_vol: 0.20,      // IV
                risk_free_rate: 0.05,   // risk-free rate
                current_time: Utc::now(),
            }
        );

        let long_call = OptionsContract::new(
            OptionsContractParams {
                underlying: "SPY".to_string(),
                option_type: OptionType::Call,
                strike: 415.0,
                expiration,
                underlying_price: 400.0,
                implied_vol: 0.20,
                risk_free_rate: 0.05,
                current_time: Utc::now(),
            }
        );

        let short_put = OptionsContract::new(
            OptionsContractParams {
                underlying: "SPY".to_string(),
                option_type: OptionType::Put,
                strike: 390.0,
                expiration,
                underlying_price: 400.0,
                implied_vol: 0.20,
                risk_free_rate: 0.05,
                current_time: Utc::now(),
            }
        );

        let long_put = OptionsContract::new(
            OptionsContractParams {
                underlying: "SPY".to_string(),
                option_type: OptionType::Put,
                strike: 385.0,
                expiration,
                underlying_price: 400.0,
                implied_vol: 0.20,
                risk_free_rate: 0.05,
                current_time: Utc::now(),
            }
        );

        let position = IronCondorPosition::new(
            "SPY".to_string(),
            short_call,
            long_call,
            short_put,
            long_put,
            1,
            now,
        );

        
        assert!(position.is_open());
        assert_eq!(position.call_spread_width(), 5.0);
        assert_eq!(position.put_spread_width(), 5.0);
        assert!(position.max_profit() > 0.0);
        assert!(position.max_loss() > 0.0);
    }
}
