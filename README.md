# uniswap_v2

Uniswap V2 `PairCreated` and `Sync` events on Ethereum mainnet, with USD values on the reserves.

## Overview

The package reads `PairCreated` from the Uniswap V2 factory at `0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f` and stores each pair address with its `token0` and `token1`. From block 25,650,000 it reads `Sync` only for those pairs. USDC is the $1 anchor. USDT, DAI, WETH, and WBTC take a liquidity-weighted USD price from their Uniswap V2 pairs against tokens that already have a price. A `Sync` gets `reserve0_usd` and `reserve1_usd` when either token has one of those prices.

## Modules

| Module | Kind | Output | Description |
|---|---|---|---|
| `map_pair_created` | map | `uniswapv2.v1.PairCreatedEvents` | `PairCreated` from the factory, from block 10,000,835 |
| `store_pairs` | store | `uniswapv2.v1.PairTokens` | Pair address to `token0` and `token1`, written once |
| `map_raw_syncs` | map | `uniswapv2.v1.SyncEvents` | `Sync` for stored pairs, from block 25,650,000, USD fields empty |
| `store_quote_reserves` | store | `uniswapv2.v1.QuoteReserves` | Latest reserves of WBTC, DAI, USDC, WETH, and USDT pairs |
| `map_syncs` | map | `uniswapv2.v1.SyncEvents` | Same `Sync` rows with `reserve0_usd` and `reserve1_usd` filled when a price exists |

## Prerequisites

- [substreams CLI](https://docs.substreams.dev/reference-material/substreams-cli/installing-the-cli) 1.20 or newer
- [buf](https://buf.build/docs/installation)
- Rust with the `wasm32-unknown-unknown` target
- A Substreams API key (`substreams auth`, or `SUBSTREAMS_API_KEY`)

## Quick Start

```bash
substreams build

# First factory pair (USDC/WETH) is in block 10008355. The store still starts at deployment, block 10000835.
substreams run substreams.yaml map_pair_created -s 10008355 -t +1 -o jsonl

# Sync events with USD reserves. The pair store backfills from block 10000835 first.
# The default processed-block guard stops that backfill, so raise it.
substreams run substreams.yaml map_syncs -s 25650000 -t +20 -o jsonl --limit-processed-blocks 0
```

`reserve0_usd` and `reserve1_usd` are decimal strings. They are empty when neither token is USDC, USDT, DAI, WETH, or WBTC, or when that quote token does not have a price yet. Quote prices are taken from reserves at the start of the block, so they trail the latest `Sync` by one block. When only one token is priced, both reserves are valued at that token's USD amount. When both are priced, each reserve uses its own price.

`pair_index` on `PairCreated` is the unnamed `uint256` in `PairCreated(address,address,address,uint256)`.
