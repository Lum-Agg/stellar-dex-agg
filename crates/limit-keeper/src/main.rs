use {
    anyhow::{Context, Result},
    dex_adapters::{
        rpc::events::{EventFilterSpec, MAX_LEDGER_SCAN_PER_REQUEST},
        SorobanRpc,
    },
    limit_keeper::{
        book::OpenOrderBook,
        config::KeeperConfig,
        events::parse_escrow_event,
        execute::{execute_fill, fill_amount, fill_min_amount_out},
        ledger::{load_checkpoint, save_checkpoint, KeeperCheckpoint},
        quote::{is_fillable_for, QuoteApiClient},
    },
    tracing::{info, warn},
    tracing_subscriber::EnvFilter,
};

fn config_path() -> Result<(Option<std::path::PathBuf>, bool, bool)> {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut check = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => path = Some(args.next().context("--config requires a file path")?.into()),
            "-h" | "--help" => {
                println!("limit-keeper --config <FILE> [--check-config]\n\n--config <FILE>  Keeper TOML configuration\n--check-config   Validate without connecting to RPC\nWithout --config, legacy environment variables are used.");
                return Ok((None, true, false));
            }
            "--check-config" => check = true,
            _ => anyhow::bail!("unknown argument: {arg}"),
        }
    }
    Ok((path, false, check))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("limit_keeper=info".parse()?))
        .init();

    let (path, help, check) = config_path()?;
    if help {
        return Ok(());
    }
    let config = match path {
        Some(path) => KeeperConfig::from_file(path)?,
        None => KeeperConfig::from_env()?,
    };
    if check {
        println!("configuration is valid");
        return Ok(());
    }
    let rpc = SorobanRpc::new(&config.rpc_url, &config.network);
    let quote_api = QuoteApiClient::new(&config.quote_api_url);
    let latest = rpc
        .get_latest_ledger()
        .await
        .context("get latest ledger at keeper startup")?
        .sequence;
    let (mut cursor, mut book) = load_checkpoint(&config.cursor_path)?
        .map(KeeperCheckpoint::into_parts)
        .unwrap_or_else(|| {
            (
                latest.saturating_sub(MAX_LEDGER_SCAN_PER_REQUEST).max(1),
                OpenOrderBook::default(),
            )
        });
    info!(
        dry_run = config.dry_run,
        cursor,
        open_orders = book.iter().count(),
        escrow = %config.escrow_contract,
        aggregator = %config.aggregator_contract,
        "limit keeper started"
    );

    loop {
        let latest = match rpc.get_latest_ledger().await {
            Ok(latest) => latest.sequence,
            Err(error) => {
                warn!(%error, "could not read latest ledger");
                tokio::time::sleep(std::time::Duration::from_secs(config.poll_secs)).await;
                continue;
            }
        };
        while cursor < latest {
            let end = cursor.saturating_add(MAX_LEDGER_SCAN_PER_REQUEST).min(latest);
            let filters = [EventFilterSpec {
                contract_ids: Some(vec![config.escrow_contract.clone()]),
                topics: None,
            }];
            let events = match rpc.get_contract_events(cursor, Some(end), &filters, 1_000).await {
                Ok(events) => events,
                Err(error) => {
                    warn!(%error, cursor, end, "failed to poll escrow events");
                    break;
                }
            };
            for event in events {
                match parse_escrow_event(&event) {
                    Ok(Some(update)) => book.apply(update),
                    Ok(None) => {}
                    Err(error) => warn!(%error, event_id = %event.id, "skipping malformed escrow event"),
                }
            }
            cursor = end;
            save_checkpoint(&config.cursor_path, &KeeperCheckpoint::capture(cursor, &book))?;
            info!(cursor, open_orders = book.iter().count(), "processed escrow events");
        }

        for order in book.iter().cloned().collect::<Vec<_>>() {
            if latest >= order.expires_ledger {
                if config.reclaim {
                    // Reclaim is intentionally not submitted in this MVP. It is
                    // safe to enable only after a dedicated reclaim transaction
                    // path is operationally tested.
                    info!(
                        order_id = order.order_id,
                        "expired order reclaim requested; skipping in MVP"
                    );
                }
                continue;
            }
            if order.next_executable_ledger.is_some_and(|due| latest < due) {
                continue;
            }
            let amount_in = fill_amount(&order, config.max_fill);
            if amount_in <= 0 {
                warn!(
                    order_id = order.order_id,
                    kind = ?order.kind,
                    "skipping order with non-positive fill amount"
                );
                continue;
            }
            let quote = match quote_api
                .fetch_quote(&order.token_in, &order.token_out, amount_in)
                .await
            {
                Ok(quote) => quote,
                Err(error) => {
                    warn!(order_id = order.order_id, %error, "quote failed");
                    continue;
                }
            };
            if !is_fillable_for(&order, amount_in, quote.expected_output) {
                continue;
            }
            let min_amount_out = fill_min_amount_out(&order, amount_in, &quote);
            if config.dry_run {
                info!(
                    order_id = order.order_id,
                    amount_in,
                    expected_out = quote.expected_output,
                    min_amount_out,
                    "dry-run: would fill escrow order"
                );
                continue;
            }
            match execute_fill(
                &config.rpc_url,
                &config.network,
                &config.secret,
                &config.escrow_contract,
                &order,
                amount_in,
                &quote,
            )
            .await
            {
                Ok(hash) => info!(order_id = order.order_id, kind = ?order.kind, %hash, "submitted escrow fill"),
                Err(error) => warn!(order_id = order.order_id, kind = ?order.kind, %error, "escrow fill failed"),
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(config.poll_secs)).await;
    }
}
