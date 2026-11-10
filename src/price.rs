//! Derived USD prices for the Uniswap V2 quote set.
//!
//! USDC is the $1 anchor. A ratio of two tokens has no USD unit until one side
//! is pinned, so USDT, DAI, WETH, and WBTC are priced from their factory pairs
//! against tokens that already have a price. Weights are the quote side's USD
//! liquidity, so a deeper pool counts for more than a thin one.

use std::collections::HashMap;
use std::str::FromStr;

use substreams::scalar::{BigDecimal, BigInt};

pub const WBTC: &str = "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599";
pub const DAI: &str = "0x6b175474e89094c44da98b954eedeac495271d0f";
pub const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
pub const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
pub const USDT: &str = "0xdac17f958d2ee523a2206206994597c13d831ec7";

/// Address-sorted. Pair token0/token1 on Uniswap V2 follow this same order.
const QUOTE_TOKENS: [&str; 5] = [WBTC, DAI, USDC, WETH, USDT];

#[derive(Clone, Debug)]
pub struct DerivedPrices {
    usdc: BigDecimal,
    usdt: Option<BigDecimal>,
    dai: Option<BigDecimal>,
    weth: Option<BigDecimal>,
    wbtc: Option<BigDecimal>,
}

impl DerivedPrices {
    pub fn for_token(&self, token: &str) -> Option<BigDecimal> {
        match token {
            USDC => Some(self.usdc.clone()),
            USDT => self.usdt.clone(),
            DAI => self.dai.clone(),
            WETH => self.weth.clone(),
            WBTC => self.wbtc.clone(),
            _ => None,
        }
    }
}

#[derive(Default)]
pub struct QuoteBook {
    /// Key is `quote_key(token0, token1)`. Reserves are in that token order.
    pairs: HashMap<String, (BigInt, BigInt)>,
}

impl QuoteBook {
    pub fn insert(&mut self, token0: &str, token1: &str, reserve0: BigInt, reserve1: BigInt) {
        self.pairs
            .insert(quote_key(token0, token1), (reserve0, reserve1));
    }

    /// Reserves of `base` and `quote` in that order.
    fn reserves_of(&self, base: &str, quote: &str) -> Option<(BigInt, BigInt)> {
        let (reserve0, reserve1) = self.pairs.get(&quote_key(base, quote))?;
        if base <= quote {
            Some((reserve0.clone(), reserve1.clone()))
        } else {
            Some((reserve1.clone(), reserve0.clone()))
        }
    }

    pub fn derive(&self) -> DerivedPrices {
        let usdc = BigDecimal::one();
        let usdt = self.price_against(USDT, &[(USDC, &usdc)]);

        let mut dai_quotes: Vec<(&str, BigDecimal)> = vec![(USDC, usdc.clone())];
        if let Some(price) = &usdt {
            dai_quotes.push((USDT, price.clone()));
        }
        let dai = self.price_against_owned(DAI, &dai_quotes);

        let mut weth_quotes: Vec<(&str, BigDecimal)> = vec![(USDC, usdc.clone())];
        if let Some(price) = &usdt {
            weth_quotes.push((USDT, price.clone()));
        }
        if let Some(price) = &dai {
            weth_quotes.push((DAI, price.clone()));
        }
        let weth = self.price_against_owned(WETH, &weth_quotes);

        let mut wbtc_quotes: Vec<(&str, BigDecimal)> = vec![(USDC, usdc)];
        if let Some(price) = &usdt {
            wbtc_quotes.push((USDT, price.clone()));
        }
        if let Some(price) = &dai {
            wbtc_quotes.push((DAI, price.clone()));
        }
        if let Some(price) = &weth {
            wbtc_quotes.push((WETH, price.clone()));
        }
        let wbtc = self.price_against_owned(WBTC, &wbtc_quotes);

        DerivedPrices {
            usdc: BigDecimal::one(),
            usdt,
            dai,
            weth,
            wbtc,
        }
    }

    fn price_against(&self, base: &str, quotes: &[(&str, &BigDecimal)]) -> Option<BigDecimal> {
        let owned: Vec<(&str, BigDecimal)> = quotes
            .iter()
            .map(|(token, price)| (*token, (*price).clone()))
            .collect();
        self.price_against_owned(base, &owned)
    }

    /// Liquidity-weighted USD price of `base`.
    /// Weight of each pool is the quote token's reserve value in USD.
    fn price_against_owned(&self, base: &str, quotes: &[(&str, BigDecimal)]) -> Option<BigDecimal> {
        let base_decimals = decimals(base)?;
        let mut numerator = BigDecimal::zero();
        let mut denominator = BigDecimal::zero();

        for (quote, quote_price) in quotes {
            let Some((reserve_base, reserve_quote)) = self.reserves_of(base, quote) else {
                continue;
            };
            if is_zero(&reserve_base) || is_zero(&reserve_quote) {
                continue;
            }
            let Some(quote_decimals) = decimals(quote) else {
                continue;
            };
            if quote_price.is_zero() {
                continue;
            }

            let base_human = reserve_base.to_decimal(base_decimals);
            let quote_human = reserve_quote.to_decimal(quote_decimals);
            if base_human.is_zero() {
                continue;
            }
            let quote_usd = quote_human * quote_price.clone();
            if quote_usd.is_zero() {
                continue;
            }
            let price = quote_usd.clone() / base_human;
            numerator += price * quote_usd.clone();
            denominator += quote_usd;
        }

        if denominator.is_zero() {
            None
        } else {
            Some(numerator / denominator)
        }
    }
}

pub fn quote_key(a: &str, b: &str) -> String {
    if a <= b {
        format!("{a}-{b}")
    } else {
        format!("{b}-{a}")
    }
}

pub fn quote_pair_keys() -> Vec<String> {
    let mut keys = Vec::new();
    for i in 0..QUOTE_TOKENS.len() {
        for j in (i + 1)..QUOTE_TOKENS.len() {
            keys.push(quote_key(QUOTE_TOKENS[i], QUOTE_TOKENS[j]));
        }
    }
    keys
}

pub fn is_quote_pair(token0: &str, token1: &str) -> bool {
    decimals(token0).is_some() && decimals(token1).is_some()
}

pub fn decimals(token: &str) -> Option<u64> {
    match token {
        USDC | USDT => Some(6),
        DAI | WETH => Some(18),
        WBTC => Some(8),
        _ => None,
    }
}

/// USD value of each reserve.
///
/// When both tokens have a derived price, each reserve is marked with its own
/// price. When only one does, both reserves are valued at that side's USD
/// amount, which is the pool's own spot. A zero reserve on the priced side
/// leaves the other side empty, because the spot ratio is undefined.
pub fn reserve_usd_values(
    token0: &str,
    token1: &str,
    reserve0: &BigInt,
    reserve1: &BigInt,
    prices: &DerivedPrices,
) -> (String, String) {
    match (prices.for_token(token0), prices.for_token(token1)) {
        (Some(price0), Some(price1)) => (
            format_usd(&usd_amount(reserve0, token0, &price0)),
            format_usd(&usd_amount(reserve1, token1, &price1)),
        ),
        (Some(price0), None) => {
            let amount = usd_amount(reserve0, token0, &price0);
            let text = format_usd(&amount);
            if is_zero(reserve0) {
                (text, String::new())
            } else {
                (text.clone(), text)
            }
        }
        (None, Some(price1)) => {
            let amount = usd_amount(reserve1, token1, &price1);
            let text = format_usd(&amount);
            if is_zero(reserve1) {
                (String::new(), text)
            } else {
                (text.clone(), text)
            }
        }
        (None, None) => (String::new(), String::new()),
    }
}

fn usd_amount(reserve: &BigInt, token: &str, price: &BigDecimal) -> BigDecimal {
    let decimals = decimals(token).expect("a priced quote token has known decimals");
    reserve.to_decimal(decimals) * price.clone()
}

/// Plain decimal with up to 8 fractional digits, half-up, trailing zeros removed.
pub fn format_usd(value: &BigDecimal) -> String {
    if value.is_zero() {
        return "0".to_string();
    }

    let factor = BigDecimal::from(100_000_000u64);
    let half = BigDecimal::one() / BigDecimal::from(2);
    let scaled = value.clone() * factor + half;
    let units = scaled.to_bigint();
    let digits = units.to_string();
    let (sign, digits) = digits
        .strip_prefix('-')
        .map(|rest| ("-", rest))
        .unwrap_or(("", digits.as_str()));

    const SCALE: usize = 8;
    let (int_part, frac) = if digits.len() <= SCALE {
        let frac = format!("{:0>width$}", digits, width = SCALE);
        ("0".to_string(), frac)
    } else {
        let split = digits.len() - SCALE;
        (digits[..split].to_string(), digits[split..].to_string())
    };
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() {
        format!("{sign}{int_part}")
    } else {
        format!("{sign}{int_part}.{frac}")
    }
}

fn is_zero(value: &BigInt) -> bool {
    value == &BigInt::from(0u32)
}

pub fn parse_int(value: &str) -> Option<BigInt> {
    BigInt::from_str(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(mantissa: &str, zeros: usize) -> BigInt {
        BigInt::from_str(&format!("{mantissa}{}", "0".repeat(zeros))).unwrap()
    }

    fn prices_with_weth(weth: &str) -> DerivedPrices {
        DerivedPrices {
            usdc: BigDecimal::one(),
            usdt: None,
            dai: None,
            weth: Some(BigDecimal::try_from(weth).unwrap()),
            wbtc: None,
        }
    }

    #[test]
    fn weth_price_from_usdc_pool_is_the_spot() {
        let mut book = QuoteBook::default();
        // 2,000,000 USDC against 1,000 WETH → 2,000 USD per WETH.
        book.insert(USDC, WETH, int("2000000", 6), int("1000", 18));
        let prices = book.derive();
        let weth = prices.for_token(WETH).unwrap();
        assert_eq!(format_usd(&weth), "2000");
        assert!(prices.for_token(USDT).is_none());
        assert!(prices.for_token(WBTC).is_none());
    }

    #[test]
    fn weth_price_is_weighted_by_quote_liquidity() {
        let mut book = QuoteBook::default();
        // USDT at par with USDC so the second pool's weight is its USDT reserve.
        book.insert(USDC, USDT, int("1000", 6), int("1000", 6));
        // 2,000,000 USDC / 1,000 WETH → 2,000, weight 2,000,000
        book.insert(USDC, WETH, int("2000000", 6), int("1000", 18));
        // 1,000,000 USDT / 400 WETH → 2,500, weight 1,000,000
        book.insert(WETH, USDT, int("400", 18), int("1000000", 6));

        let weth = book.derive().for_token(WETH).unwrap();
        // (2000 * 2e6 + 2500 * 1e6) / 3e6 = 2166.666...
        assert_eq!(format_usd(&weth), "2166.66666667");
    }

    #[test]
    fn usdt_price_comes_from_the_usdc_pool() {
        let mut book = QuoteBook::default();
        book.insert(USDC, USDT, int("1000", 6), int("1010", 6));
        let usdt = book.derive().for_token(USDT).unwrap();
        assert_eq!(format_usd(&usdt), "0.99009901");
    }

    #[test]
    fn both_priced_reserves_use_their_own_prices() {
        let prices = prices_with_weth("2100");
        let (usd0, usd1) = reserve_usd_values(
            USDC,
            WETH,
            &int("2000000", 6),
            &int("1000", 18),
            &prices,
        );
        assert_eq!(usd0, "2000000");
        assert_eq!(usd1, "2100000");
    }

    #[test]
    fn one_priced_side_values_both_reserves_at_the_pool_spot() {
        let prices = prices_with_weth("2000");
        let other = "0x0000000000000000000000000000000000000001";
        let (usd0, usd1) = reserve_usd_values(other, WETH, &int("500", 18), &int("1", 18), &prices);
        assert_eq!(usd0, "2000");
        assert_eq!(usd1, "2000");
    }

    #[test]
    fn zero_priced_reserve_does_not_invent_the_other_side() {
        let prices = prices_with_weth("2000");
        let other = "0x0000000000000000000000000000000000000001";
        let (usd0, usd1) = reserve_usd_values(WETH, other, &BigInt::from(0u32), &int("5", 18), &prices);
        assert_eq!(usd0, "0");
        assert_eq!(usd1, "");
    }

    #[test]
    fn unpriced_pair_leaves_usd_empty() {
        let prices = prices_with_weth("2000");
        let (usd0, usd1) = reserve_usd_values(
            "0x0000000000000000000000000000000000000001",
            "0x0000000000000000000000000000000000000002",
            &int("1", 18),
            &int("1", 18),
            &prices,
        );
        assert_eq!(usd0, "");
        assert_eq!(usd1, "");
    }

    #[test]
    fn zero_reserve_pool_is_ignored() {
        let mut book = QuoteBook::default();
        book.insert(USDC, WETH, int("2000000", 6), BigInt::from(0u32));
        assert!(book.derive().for_token(WETH).is_none());
    }
}
