use std::{collections::HashMap, mem::size_of, str::FromStr};

use anyhow::anyhow;
use ellipsis_client::EllipsisClient;
use phoenix::{
    program::{load_with_dispatch, MarketHeader},
    quantities::WrapperU64,
};
use phoenix_sdk::sdk_client::SDKClient;
use serde_json::Value;
use solana_sdk::pubkey::Pubkey;

use super::process_get_all_markets::{get_base_and_quote_symbols, get_phoenix_config};

pub async fn process_get_uncollected_revenue(
    client: &EllipsisClient,
    network_url: &str,
) -> anyhow::Result<()> {
    let config = get_phoenix_config(client).await?;
    let markets = config
        .markets
        .iter()
        .map(|m| m.market.clone())
        .collect::<Vec<String>>()
        .clone();

    let mut sdk = SDKClient::new(&client.payer, network_url).await?;

    let usdtprice = get_price("USDT", "USDC").await?;
    let solprice = get_price("SOL", "USDC").await?;

    println!("Retrieving current balances...");
    let mut total_usdc = 0f32;
    let mut total_usdt = 0f32;
    let mut total_sol = 0f32;
    let mut total = 0f32;
    for market_key in markets {
        let market_pubkey = &Pubkey::from_str(&market_key)?;
        sdk.add_market(&market_pubkey).await?;
        let market_metadata = sdk.get_market_metadata(market_pubkey).await?;

        let market_account_data = sdk.client.get_account_data(&market_pubkey).await?;
        let (header_bytes, market_bytes) = market_account_data.split_at(size_of::<MarketHeader>());
        let header: &MarketHeader = bytemuck::try_from_bytes(header_bytes)
            .map_err(|e| anyhow::anyhow!("Error getting market header. Error: {:?}", e))?;

        let market = load_with_dispatch(&header.market_size_params, market_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to load market. Error {:?}", e))?
            .inner;

        let (_, quote_mint_symbol) = get_base_and_quote_symbols(&config, header);
        let quote_mint_symbol = quote_mint_symbol.unwrap();
        let quote_mint_symbol = quote_mint_symbol.as_str();

        let amt = market.get_uncollected_fee_amount().as_u64() as f32
            / 10f32.powi(market_metadata.quote_decimals as i32);
        match quote_mint_symbol {
            "USDC" => {
                total_usdc += amt;
                total += amt;
            }
            "USDT" => {
                total_usdt += amt;
                total += usdtprice * amt;
            }
            "SOL" => {
                total_sol += amt;
                total += solprice * amt;
            }
            _ => return Err(anyhow!(
                "The {market_key} market is using an unsupported quote token: {quote_mint_symbol}."
            )),
        }
    }
    println!("USDC: {total_usdc}");
    println!("USDT: {total_usdt}");
    println!("SOL: {total_sol}");
    println!("Total (USDC): {total}");
    Ok(())
}

async fn get_price(symbol_a: &str, symbol_b: &str) -> anyhow::Result<f32> {
    let body = reqwest::get(format!(
        "https://api.coinbase.com/v2/prices/{symbol_a}-{symbol_b}/spot"
    ))
    .await.map_err(|_| anyhow!("Failed to get price data, looks like Coinbase is down.."))?
    .json::<HashMap<String, Value>>()
    .await?;
    let price = &body["data"]["amount"].as_str().unwrap(); //fails if coinbase changes their format
    price
        .parse::<f32>()
        .map_err(|e| anyhow!("Failed to get price, Error {e}"))
}
