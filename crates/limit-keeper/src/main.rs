use {
    anyhow::{Context, Result},
    dex_adapters::{
        rpc::events::{EventFilterSpec, MAX_LEDGER_SCAN_PER_REQUEST},
        SorobanRpc,
    },
    limit_keeper::{
        book::OpenOrderBook,
        config::KeeperConfig,
        events::apply_escrow_batch,
        execute::{execute_fill, fill_amount, fill_min_amount_out, VenueChecks},
        ledger::{checkpoint_path, load_checkpoint, save_checkpoint, KeeperCheckpoint},
        quote::{is_fillable_for, QuoteApiClient},
    },
    tracing::{error, info, warn},
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
    let cursor_path = checkpoint_path(&config.cursor_path, &config.network, &config.escrow_contract)?;
    if check {
        println!("configuration is valid");
        return Ok(());
    }
    let rpc = SorobanRpc::new(&config.rpc_url, &config.network);
    let quote_api = QuoteApiClient::new(&config.quote_api_url);
    let (_, latest) = rpc
        .get_history_bounds()
        .await
        .context("read RPC history bounds at keeper startup")?;
    let (mut cursor, mut book) = match load_checkpoint(&cursor_path)? {
        Some(checkpoint) => checkpoint.into_parts(),
        None => {
            let start = config.start_ledger.unwrap_or_else(|| {
                warn!("no keeper start_ledger configured; initial scan uses only the recent ledger window");
                latest.saturating_sub(MAX_LEDGER_SCAN_PER_REQUEST).max(1)
            });
            (start, OpenOrderBook::default())
        }
    };
    info!(
        dry_run = config.dry_run,
        cursor,
        checkpoint = %cursor_path.display(),
        open_orders = book.iter().count(),
        escrow = %config.escrow_contract,
        aggregator = %config.aggregator_contract,
        "limit keeper started"
    );

    let poll_delay = std::time::Duration::from_secs(config.poll_secs);
    let gap_delay = std::time::Duration::from_secs(config.poll_secs.max(60));
    let mut waiting_for_history = false;
    'poll: loop {
        let (oldest, latest) = match rpc.get_history_bounds().await {
            Ok(bounds) => bounds,
            Err(error) => {
                warn!(%error, "keeper paused: could not read RPC history bounds");
                tokio::time::sleep(poll_delay).await;
                continue;
            }
        };
        if cursor < oldest {
            if !waiting_for_history {
                error!(
                    cursor,
                    oldest,
                    latest,
                    checkpoint = %cursor_path.display(),
                    "keeper paused: RPC history gap; use an RPC that serves the checkpoint ledger, do not skip ahead"
                );
            }
            waiting_for_history = true;
            tokio::time::sleep(gap_delay).await;
            continue;
        }
        if cursor > latest {
            warn!(cursor, latest, "keeper paused: RPC is behind the checkpoint");
            tokio::time::sleep(poll_delay).await;
            continue;
        }
        if waiting_for_history {
            info!(cursor, oldest, latest, "keeper history available again; resuming event replay");
            waiting_for_history = false;
        }
        while cursor < latest {
            let end = cursor.saturating_add(MAX_LEDGER_SCAN_PER_REQUEST).min(latest);
            let filters = [EventFilterSpec {
                contract_ids: Some(vec![config.escrow_contract.clone()]),
                topics: None,
            }];
            let events = match rpc.get_contract_events(cursor, Some(end), &filters, 1_000).await {
                Ok(events) => events,
                Err(error) => {
                    warn!(%error, cursor, end, "keeper paused: failed to sync escrow events");
                    tokio::time::sleep(poll_delay).await;
                    continue 'poll;
                }
            };
            if let Err(error) = apply_escrow_batch(&mut book, &events) {
                warn!(error = %format_args!("{error:#}"), cursor, end, "keeper paused: failed to decode escrow event batch");
                tokio::time::sleep(poll_delay).await;
                continue 'poll;
            }
            cursor = end;
            save_checkpoint(&cursor_path, &KeeperCheckpoint::capture(cursor, &book))?;
            info!(cursor, open_orders = book.iter().count(), "processed escrow events");
        }

        let mut venues = VenueChecks::new(&rpc, &config.aggregator_contract);
        for order in book.iter().cloned().collect::<Vec<_>>() {
            if latest >= order.expires_ledger {
                if config.reclaim {
                    if config.dry_run {
                        info!(order_id = order.order_id, kind = ?order.kind, "dry-run: would reclaim expired escrow order");
                    } else {
                        match limit_keeper::execute::execute_reclaim(
                            &config.rpc_url,
                            &config.network,
                            &config.secret,
                            &config.escrow_contract,
                            order.kind,
                            order.order_id,
                        )
                        .await
                        {
                            Ok(hash) => {
                                info!(order_id = order.order_id, kind = ?order.kind, %hash, "submitted expired escrow reclaim");
                                book.apply_expired(order.kind, order.order_id);
                                save_checkpoint(&cursor_path, &KeeperCheckpoint::capture(cursor, &book))?;
                            }
                            Err(error) => {
                                warn!(order_id = order.order_id, kind = ?order.kind, %error, "expired escrow reclaim failed")
                            }
                        }
                    }
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
            let Some(min_amount_out) = fill_min_amount_out(&order, amount_in, &quote) else {
                continue;
            };
            if let Err(error) = venues.verify_quote(&quote).await {
                warn!(order_id = order.order_id, %error, "skipping fill: venue preflight failed");
                continue;
            }
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
        tokio::time::sleep(poll_delay).await;
    }
}
