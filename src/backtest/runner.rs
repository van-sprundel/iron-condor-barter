use chrono::{DateTime, Duration, Utc};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::info;
use uuid::Uuid;

use crate::backtest::metrics::{BacktestMetrics, Trade};
use crate::models::options_data::OptionsChain;
use crate::strategies::iron_condor::{IronCondorSignal, IronCondorSignalGenerator};

/// Configuration for a backtest run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    /// Initial capital
    pub initial_capital: f64,
    /// Start date for backtest
    pub start_date: DateTime<Utc>,
    /// End date for backtest
    pub end_date: DateTime<Utc>,
    /// Trading commission per contract
    pub commission_per_contract: f64,
    /// Slippage model settings (percentage)
    pub slippage_pct: f64,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            initial_capital: 100_000.0,
            start_date: Utc::now() - Duration::days(365),
            end_date: Utc::now(),
            commission_per_contract: 0.65,
            slippage_pct: 0.05,
        }
    }
}

/// Enhanced market data event with options chains
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedMarketEvent {
    /// Symbol
    pub symbol: String,
    /// Underlying price
    pub underlying_price: f64,
    /// Volume
    pub volume: f64,
    /// Implied volatility (VIX-style)
    pub implied_volatility: f64,
    /// Options chains for different expirations
    pub options_chains: HashMap<String, OptionsChain>, // expiration_date_string -> chain
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl EnhancedMarketEvent {
    /// Get options chain for a specific expiration
    pub fn get_options_chain(&self, expiration_date: &str) -> Option<&OptionsChain> {
        self.options_chains.get(expiration_date)
    }

    /// Get the nearest expiration date
    pub fn get_nearest_expiration(&self) -> Option<String> {
        self.options_chains.keys().min_by(|a, b| a.cmp(b)).cloned()
    }

    /// Update all options chains with new underlying price
    #[allow(dead_code)]
    pub fn update_options_chains(
        &mut self,
        new_underlying_price: f64,
        new_timestamp: DateTime<Utc>,
    ) {
        self.underlying_price = new_underlying_price;
        self.timestamp = new_timestamp;

        for chain in self.options_chains.values_mut() {
            chain.update(new_underlying_price, new_timestamp);
        }
    }
}

/// Market data generator for backtesting with options chains
pub struct HistoricalMarketGenerator {
    /// Symbol
    #[allow(dead_code)]
    pub symbol: String,
    /// Enhanced market events data with options
    pub events: Vec<EnhancedMarketEvent>,
    /// Current index
    pub current_idx: usize,
}

impl HistoricalMarketGenerator {
    /// Create a new historical market generator with enhanced options data
    pub fn new(symbol: String, events: Vec<EnhancedMarketEvent>) -> Self {
        Self {
            symbol,
            events,
            current_idx: 0,
        }
    }

    /// Get next market event with options data
    pub async fn next_event(&mut self) -> Option<EnhancedMarketEvent> {
        if self.current_idx >= self.events.len() {
            return None;
        }

        let event = self.events[self.current_idx].clone();
        self.current_idx += 1;

        Some(event)
    }
}

impl Stream for HistoricalMarketGenerator {
    type Item = EnhancedMarketEvent;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.current_idx >= self.events.len() {
            return Poll::Ready(None);
        }

        let event = self.events[self.current_idx].clone();
        self.current_idx += 1;

        Poll::Ready(Some(event))
    }
}

/// Runs a backtest for a given strategy and data
pub struct BacktestRunner {
    /// Backtest configuration
    pub config: BacktestConfig,
    /// Market data generator
    pub market_generator: HistoricalMarketGenerator,
    /// Strategy signal generator
    pub strategy: IronCondorSignalGenerator,
    /// Trades executed during backtest
    pub trades: Vec<Trade>,
    /// Current capital
    pub current_capital: f64,
    /// Equity curve (timestamp -> equity)
    pub equity_curve: HashMap<DateTime<Utc>, f64>,
}

impl BacktestRunner {
    /// Create a new backtest runner
    pub fn new(
        config: BacktestConfig,
        market_generator: HistoricalMarketGenerator,
        strategy: IronCondorSignalGenerator,
    ) -> Self {
        Self {
            config: config.clone(),
            market_generator,
            strategy,
            trades: Vec::new(),
            current_capital: config.initial_capital,
            equity_curve: HashMap::new(),
        }
    }

    /// Run the backtest
    pub async fn run(&mut self) -> BacktestMetrics {
        info!(
            "Starting backtest from {} to {}",
            self.config.start_date, self.config.end_date
        );

        // Simple backtest simulation - just process market events
        let mut current_capital = self.config.initial_capital;
        let mut trades = Vec::new();
        let mut equity_curve = HashMap::new();

        // Process all market events
        let mut event_count = 0;
        let mut active_trades: HashMap<Uuid, Trade> = HashMap::new();

        while let Some(event) = self.market_generator.next_event().await {
            event_count += 1;

            // Update equity curve with current capital (mark-to-market)
            equity_curve.insert(event.timestamp, current_capital);

            // Use the nearest expiration options chain for signal generation
            if let Some(expiration_key) = event.get_nearest_expiration() {
                if let Some(options_chain) = event.get_options_chain(&expiration_key) {
                    if let Some(signal) = self
                        .strategy
                        .generate_signal_with_options_chain(options_chain)
                    {
                        match signal {
                            IronCondorSignal::Enter {
                                position,
                                timestamp,
                            } => {
                                let trade = Trade {
                                    id: position.id,
                                    symbol: event.symbol.clone(),
                                    entry_price: position.entry_premium, // Use premium as "price"
                                    exit_price: 0.0,                     // Will be set on exit
                                    quantity: position.quantity,
                                    entry_time: timestamp,
                                    exit_time: timestamp, // Will be updated on exit
                                    trade_type: "IronCondor".to_string(),
                                    metadata: serde_json::json!({
                                        "entry_premium": position.entry_premium,
                                        "max_profit": position.max_profit(),
                                        "max_loss": position.max_loss(),
                                        "short_call_strike": position.short_call.strike,
                                        "long_call_strike": position.long_call.strike,
                                        "short_put_strike": position.short_put.strike,
                                        "long_put_strike": position.long_put.strike,
                                        "underlying_price": options_chain.underlying_price,
                                        "status": "open"
                                    }),
                                };

                                // Apply premium immediately for credit spreads
                                let commission = self.config.commission_per_contract
                                    * position.quantity as f64
                                    * 4.0; // 4 legs
                                let net_premium = position.entry_premium - commission;
                                current_capital += net_premium;

                                active_trades.insert(position.id, trade);

                                info!(
                                    "Iron Condor ENTRY: ID={}, Premium=${:.2}, Net=${:.2} (after commission)",
                                    position.id, position.entry_premium, net_premium
                                );
                            }
                            IronCondorSignal::Exit {
                                position_id,
                                exit_premium,
                                timestamp,
                                reason,
                            } => {
                                // Find and close the corresponding trade
                                if let Some(mut trade) = active_trades.remove(&position_id) {
                                    trade.exit_price = exit_premium;
                                    trade.exit_time = timestamp;

                                    // Calculate P&L (entry premium - exit premium)
                                    let pnl = trade.entry_price - trade.exit_price;
                                    let commission = self.config.commission_per_contract
                                        * trade.quantity as f64
                                        * 4.0; // 4 legs
                                    let net_pnl = pnl - commission;

                                    current_capital += net_pnl;
                                    trades.push(trade);

                                    info!(
                                        "Iron Condor EXIT: ID={}, Exit Premium=${:.2}, P&L=${:.2}, Reason={}",
                                        position_id, exit_premium, net_pnl, reason
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Move any remaining open trades to final trades list
        for (_, mut trade) in active_trades {
            // Mark as still open
            trade.metadata.as_object_mut().unwrap().insert(
                "status".to_string(),
                serde_json::Value::String("open".to_string()),
            );
            trades.push(trade);
        }

        info!(
            "Processed {} market events, executed {} trades",
            event_count,
            trades.len()
        );

        self.trades = trades;
        self.current_capital = current_capital;
        self.equity_curve = equity_curve;

        let mut metrics = BacktestMetrics::new(self.config.initial_capital);

        let days_in_backtest = (self.config.end_date - self.config.start_date).num_days() as f64;

        metrics.calculate(self.current_capital, &self.trades, days_in_backtest);

        info!(
            "Backtest completed. Final capital: ${:.2}",
            self.current_capital
        );

        metrics
    }
}
