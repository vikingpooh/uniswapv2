mod abi;
mod price;

use price::{
    is_quote_pair, parse_int, quote_key, quote_pair_keys, reserve_usd_values, QuoteBook,
};
use substreams::errors::Error;
use substreams::prelude::{StoreSetIfNotExistsProto, StoreSetProto};
use substreams::store::{StoreGet, StoreGetProto, StoreNew, StoreSet, StoreSetIfNotExists};
use substreams::Hex;
use substreams_ethereum::pb::eth::v2::Block;
use substreams_ethereum::Event;

use crate::pb::uniswapv2::v1::{
    PairCreated, PairCreatedEvents, PairTokens, QuoteReserves, Sync, SyncEvents,
};

mod pb {
    pub mod uniswapv2 {
        pub mod v1 {
            #![allow(unused_imports)]
            include!(concat!(env!("OUT_DIR"), "/uniswapv2.v1.mod.rs"));
        }
    }
}

const FACTORY: [u8; 20] = hex_literal::hex!("5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f");
const SYNC_TOPIC: [u8; 32] =
    hex_literal::hex!("1c411e9a96e071241c2f21f7726b17ae89e3cab4c78be50e062b03a9fffbbad1");

fn hex0x(bytes: &[u8]) -> String {
    format!("0x{}", Hex::encode(bytes))
}

#[substreams::handlers::map]
pub fn map_pair_created(block: Block) -> Result<PairCreatedEvents, Error> {
    substreams::skip_empty_output();

    let timestamp = block.timestamp_seconds();
    let mut pairs = Vec::new();

    for trx in block.transactions() {
        let tx_hash = hex0x(&trx.hash);
        for (log, _call) in trx.logs_with_calls() {
            if log.address != FACTORY {
                continue;
            }
            let Some(created) = abi::factory::events::PairCreated::match_and_decode(log) else {
                continue;
            };
            pairs.push(PairCreated {
                block_number: block.number,
                timestamp,
                tx_hash: tx_hash.clone(),
                log_index: log.index,
                token0: hex0x(&created.token0),
                token1: hex0x(&created.token1),
                pair: hex0x(&created.pair),
                pair_index: created.pair_index.to_string(),
            });
        }
    }

    Ok(PairCreatedEvents { pairs })
}

#[substreams::handlers::store]
pub fn store_pairs(events: PairCreatedEvents, store: StoreSetIfNotExistsProto<PairTokens>) {
    for pair in events.pairs {
        store.set_if_not_exists(
            pair.log_index as u64,
            &pair.pair,
            &PairTokens {
                token0: pair.token0,
                token1: pair.token1,
            },
        );
    }
}

#[substreams::handlers::map]
pub fn map_raw_syncs(
    block: Block,
    created: PairCreatedEvents,
    pairs: StoreGetProto<PairTokens>,
) -> Result<SyncEvents, Error> {
    substreams::skip_empty_output();

    let mut created_this_block = std::collections::HashMap::new();
    for pair in created.pairs {
        created_this_block.insert(pair.pair, (pair.token0, pair.token1));
    }

    let timestamp = block.timestamp_seconds();
    let mut syncs = Vec::new();

    for trx in block.transactions() {
        let tx_hash = hex0x(&trx.hash);
        for (log, _call) in trx.logs_with_calls() {
            if log.topics.is_empty() || log.topics[0] != SYNC_TOPIC {
                continue;
            }
            let pair = hex0x(&log.address);
            let tokens = if let Some(tokens) = created_this_block.get(&pair) {
                tokens.clone()
            } else if let Some(tokens) = pairs.get_last(&pair) {
                (tokens.token0, tokens.token1)
            } else {
                continue;
            };

            let Some(synced) = abi::pair::events::Sync::match_and_decode(log) else {
                continue;
            };
            syncs.push(Sync {
                block_number: block.number,
                timestamp,
                tx_hash: tx_hash.clone(),
                log_index: log.index,
                pair,
                token0: tokens.0,
                token1: tokens.1,
                reserve0: synced.reserve0.to_string(),
                reserve1: synced.reserve1.to_string(),
                reserve0_usd: String::new(),
                reserve1_usd: String::new(),
            });
        }
    }

    Ok(SyncEvents { syncs })
}

#[substreams::handlers::store]
pub fn store_quote_reserves(syncs: SyncEvents, store: StoreSetProto<QuoteReserves>) {
    for sync in syncs.syncs {
        if !is_quote_pair(&sync.token0, &sync.token1) {
            continue;
        }
        store.set(
            sync.log_index as u64,
            quote_key(&sync.token0, &sync.token1),
            &QuoteReserves {
                token0: sync.token0,
                token1: sync.token1,
                reserve0: sync.reserve0,
                reserve1: sync.reserve1,
            },
        );
    }
}

#[substreams::handlers::map]
pub fn map_syncs(
    syncs: SyncEvents,
    quotes: StoreGetProto<QuoteReserves>,
) -> Result<SyncEvents, Error> {
    substreams::skip_empty_output();

    let mut book = QuoteBook::default();
    for key in quote_pair_keys() {
        let Some(reserves) = quotes.get_last(&key) else {
            continue;
        };
        let (Some(reserve0), Some(reserve1)) =
            (parse_int(&reserves.reserve0), parse_int(&reserves.reserve1))
        else {
            continue;
        };
        book.insert(&reserves.token0, &reserves.token1, reserve0, reserve1);
    }
    let prices = book.derive();

    let mut priced = Vec::with_capacity(syncs.syncs.len());
    for sync in syncs.syncs {
        let (reserve0_usd, reserve1_usd) =
            match (parse_int(&sync.reserve0), parse_int(&sync.reserve1)) {
                (Some(reserve0), Some(reserve1)) => {
                    reserve_usd_values(&sync.token0, &sync.token1, &reserve0, &reserve1, &prices)
                }
                _ => (String::new(), String::new()),
            };
        priced.push(Sync {
            reserve0_usd,
            reserve1_usd,
            ..sync
        });
    }

    Ok(SyncEvents { syncs: priced })
}
