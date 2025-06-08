mod backtest;
mod data;
mod models;
mod strategies;

use backtest::runner::{BacktestConfig, BacktestRunner, HistoricalMarketGenerator};
use chrono::{Duration, Utc};
use data::alpha_vantage::AlphaVantageClient;
use dotenv::dotenv;
use strategies::iron_condor::{IronCondorConfig, IronCondorSignalGenerator};
use tracing::{Level, info, warn};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    _ = dotenv();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");

    info!("Starting paper trading engine for options backtesting");

    let events = load_or_generate_options_data().await?;

    let symbol = events
        .first()
        .map(|e| e.symbol.clone())
        .unwrap_or_else(|| "SPY".to_string());

    let market_generator = HistoricalMarketGenerator::new(symbol.clone(), events);

    let iron_condor_config = IronCondorConfig {
        symbol: symbol.clone(),
        dte_threshold: 1,        // 0DTE
        width_percentage: 0.01,  // 1% width between strikes
        delta_target: 0.10,      // 10 delta for short strikes
        profit_target_pct: 0.50, // 50% profit target
        stop_loss_pct: 0.75,     // 75% stop loss
        exit_dte: 0,             // Hold till expiration (0DTE)
        zero_dte: true,          // 0DTE strategy
    };

    let signal_generator = IronCondorSignalGenerator::new(iron_condor_config);

    let backtest_config = BacktestConfig {
        initial_capital: 100_000.0,
        start_date: Utc::now() - Duration::days(365 * 3),
        end_date: Utc::now(),
        commission_per_contract: 0.65,
        slippage_pct: 0.03,
    };

    let mut backtest_runner =
        BacktestRunner::new(backtest_config, market_generator, signal_generator);
    let metrics = backtest_runner.run().await;

    info!("===== Backtest Results =====");
    info!("Initial Capital: ${:.2}", metrics.initial_capital);
    info!("Final Capital: ${:.2}", metrics.final_capital);
    info!("Total Return: {:.2}%", metrics.total_return_pct);
    info!("Annualized Return: {:.2}%", metrics.annualized_return_pct);
    info!("Max Drawdown: {:.2}%", metrics.max_drawdown_pct);
    info!("Sharpe Ratio: {:.2}", metrics.sharpe_ratio);
    info!("Sortino Ratio: {:.2}", metrics.sortino_ratio);
    info!("Total Trades: {}", metrics.total_trades);
    info!("Win Rate: {:.2}%", metrics.win_rate_pct);
    info!("Profit Factor: {:.2}", metrics.profit_factor);
    info!("Avg Holding Period: {:.2} days", metrics.avg_holding_days);
    info!("=============================");

    Ok(())
}

/// Load options data from alpha vantage
async fn load_or_generate_options_data()
-> Result<Vec<backtest::runner::EnhancedMarketEvent>, Box<dyn std::error::Error>> {
    info!("Attempting to fetch live options data from Alpha Vantage...");

    if let Ok(api_key) = std::env::var("ALPHA_VANTAGE_API_KEY") {
        let alpha_client = AlphaVantageClient::new(api_key);

        let mut option_chains = Vec::new();
        let tickers = [
            "SPY", "QQQ", "XRT", "XBI", "EWZ", "XOP", "FXI", "XLP", "XLE",
        ];

        for ticker in tickers {
            match alpha_client.fetch_options_chain(ticker).await {
                Ok(live_event) => {
                    info!("Successfully fetched live SPY options data from Alpha Vantage!");
                    info!("Underlying price: ${:.2}", live_event.underlying_price);
                    info!("Available expirations: {}", live_event.options_chains.len());
                    info!(
                        "Total options contracts: {}",
                        live_event
                            .options_chains
                            .values()
                            .map(|c| c.calls.len() + c.puts.len())
                            .sum::<usize>()
                    );

                    option_chains.extend(vec![live_event]);
                }
                Err(e) => {
                    warn!("Failed to fetch from Alpha Vantage: {}", e);
                    continue;
                }
            }
        }
        Ok(option_chains)
    } else {
        Err("Alpha Vantage API key not found in environment variables".into())
    }
}
