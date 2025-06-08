use serde::{Deserialize, Serialize};

/// Backtesting results and performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestMetrics {
    /// Initial capital
    pub initial_capital: f64,
    /// Final capital
    pub final_capital: f64,
    /// Total return percentage
    pub total_return_pct: f64,
    /// Annualized return percentage
    pub annualized_return_pct: f64,
    /// Maximum drawdown percentage
    pub max_drawdown_pct: f64,
    /// Sharpe ratio
    pub sharpe_ratio: f64,
    /// Sortino ratio
    pub sortino_ratio: f64,
    /// Total number of trades
    pub total_trades: usize,
    /// Number of winning trades
    pub winning_trades: usize,
    /// Number of losing trades
    pub losing_trades: usize,
    /// Win rate percentage
    pub win_rate_pct: f64,
    /// Average profit per winning trade
    pub avg_profit_per_win: f64,
    /// Average loss per losing trade
    pub avg_loss_per_loss: f64,
    /// Profit factor (gross profit / gross loss)
    pub profit_factor: f64,
    /// Average holding period in days
    pub avg_holding_days: f64,
}

impl BacktestMetrics {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            initial_capital,
            final_capital: initial_capital,
            total_return_pct: 0.0,
            annualized_return_pct: 0.0,
            max_drawdown_pct: 0.0,
            sharpe_ratio: 0.0,
            sortino_ratio: 0.0,
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
            win_rate_pct: 0.0,
            avg_profit_per_win: 0.0,
            avg_loss_per_loss: 0.0,
            profit_factor: 0.0,
            avg_holding_days: 0.0,
        }
    }

    /// Calculate metrics from backtest results
    pub fn calculate(&mut self, final_capital: f64, trades: &[Trade], days_in_backtest: f64) {
        self.final_capital = final_capital;

        self.total_return_pct =
            (final_capital - self.initial_capital) / self.initial_capital * 100.0;

        self.annualized_return_pct =
            ((final_capital / self.initial_capital).powf(365.0 / days_in_backtest) - 1.0) * 100.0;

        self.total_trades = trades.len();

        let mut total_profit = 0.0;
        let mut total_loss = 0.0;
        let mut total_holding_days = 0.0;
        let mut max_equity = self.initial_capital;
        let mut max_drawdown = 0.0;
        let mut daily_returns = Vec::new();

        for trade in trades {
            let pnl = trade.exit_price - trade.entry_price;
            let profit = pnl * trade.quantity as f64 * 100.0;

            if profit > 0.0 {
                self.winning_trades += 1;
                total_profit += profit;
            } else {
                self.losing_trades += 1;
                total_loss += profit.abs();
            }

            let holding_days = (trade.exit_time - trade.entry_time).num_days() as f64;
            total_holding_days += holding_days;

            let equity_after_trade = max_equity + profit;
            if equity_after_trade > max_equity {
                max_equity = equity_after_trade;
            } else {
                let current_drawdown = (max_equity - equity_after_trade) / max_equity * 100.0;
                if current_drawdown > max_drawdown {
                    max_drawdown = current_drawdown;
                }
            }

            let daily_return = profit / self.initial_capital / holding_days;
            daily_returns.push(daily_return);
        }

        self.win_rate_pct = if self.total_trades > 0 {
            self.winning_trades as f64 / self.total_trades as f64 * 100.0
        } else {
            0.0
        };

        self.avg_profit_per_win = if self.winning_trades > 0 {
            total_profit / self.winning_trades as f64
        } else {
            0.0
        };

        self.avg_loss_per_loss = if self.losing_trades > 0 {
            total_loss / self.losing_trades as f64
        } else {
            0.0
        };

        self.profit_factor = if total_loss > 0.0 {
            total_profit / total_loss
        } else if total_profit > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        self.avg_holding_days = if self.total_trades > 0 {
            total_holding_days / self.total_trades as f64
        } else {
            0.0
        };

        self.max_drawdown_pct = max_drawdown;

        if !daily_returns.is_empty() {
            let mean_return = daily_returns.iter().sum::<f64>() / daily_returns.len() as f64;

            // Calculate standard deviation
            let variance = daily_returns
                .iter()
                .map(|r| (r - mean_return).powi(2))
                .sum::<f64>()
                / daily_returns.len() as f64;
            let std_dev = variance.sqrt();

            // Calculate downside deviation
            let downside_returns: Vec<f64> = daily_returns
                .iter()
                .filter(|&&r| r < 0.0).copied()
                .collect();

            let downside_deviation = if !downside_returns.is_empty() {
                let downside_variance = downside_returns.iter().map(|r| r.powi(2)).sum::<f64>()
                    / downside_returns.len() as f64;
                downside_variance.sqrt()
            } else {
                0.0
            };

            // Risk-free rate (assumed 0%)
            let risk_free_rate = 0.0;

            self.sharpe_ratio = if std_dev > 0.0 {
                (mean_return - risk_free_rate) / std_dev * (252.0_f64).sqrt()
            } else {
                0.0
            };

            self.sortino_ratio = if downside_deviation > 0.0 {
                (mean_return - risk_free_rate) / downside_deviation * (252.0_f64).sqrt()
            } else if mean_return > risk_free_rate {
                f64::INFINITY
            } else {
                0.0
            };
        }
    }
}

/// Trade record for backtest analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    /// Trade ID
    pub id: uuid::Uuid,
    /// Symbol
    pub symbol: String,
    /// Entry price
    pub entry_price: f64,
    /// Exit price
    pub exit_price: f64,
    /// Quantity
    pub quantity: u32,
    /// Entry time
    pub entry_time: chrono::DateTime<chrono::Utc>,
    /// Exit time
    pub exit_time: chrono::DateTime<chrono::Utc>,
    /// Trade type (e.g., "Iron Condor")
    pub trade_type: String,
    /// Additional metadata
    pub metadata: serde_json::Value,
}
