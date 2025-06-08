# Barter Iron Condor

This was a weekend project setting up a simple strategy using [Barter](https://github.com/barter-rs/barter-rs) with the goal of understanding how the crate works.

It's importing option chains with 0DTE for backtesting. It's designed to test option strategies (currently focused on iron condors) with a goal similar to Option Alpha's 0DTE Backtester. I'm pulling data from AlphaVantage and testing against that.

While I already had a bit of knowledge regarding Iron Condors, I tripped on multiple terminology. For example, I had no idea what candles were in option chains. Safe to say I learned more than just programming :)

## Resources
- [Option Alpha Data Feeds](https://optionalpha.com/help/data-feeds)
- [AlphaVantage](https://www.alphavantage.co/)
- [Barter](https://github.com/barter-rs/barter-rs)

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
