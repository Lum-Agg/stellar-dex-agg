use kanatoko::soroban_env_host::xdr::ScError;
use kanatoko::soroban_sdk::{token::TokenClient, vec, Address, Vec};
use kanatoko::{mainnet, AutoRunner, ScenarioFork};

// Generated clients retain the deployed contract's argument lists.
#[allow(clippy::too_many_arguments)]
mod aggregator {
    use kanatoko::soroban_sdk;
    soroban_sdk::contractimport!(file = "wasm/aggregator_contract.wasm");
}

const ROUTER: &str = "CAG5LRYQ5JVEUI5TEID72EYOVX44TTUJT5BQR2J6J77FH65PCCFAJDDH";
const XLM: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const USDC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";

fn runner(name: &str) -> AutoRunner {
    let runner = mainnet().cache(format!(
        "{}/fixtures/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    ));
    match std::env::var("KANATOKO_CAPTURE_RPC") {
        Ok(url) => runner.rpc_url(url).rpc_rate_limit(1).refresh(),
        Err(_) => runner.offline(),
    }
}

struct Setup {
    user: Address,
    aggregator: Address,
    pair: Address,
    input: Address,
    output: Address,
    input_index: u32,
    leg_amount: i128,
}

#[derive(Debug, PartialEq)]
struct State {
    // user, aggregator, pair; input then output for each.
    balances: [i128; 6],
    reserves: (i128, i128),
}

fn setup(fork: &ScenarioFork<'_>) -> Setup {
    fork.mock_all_auths();
    let input = fork.contract(XLM);
    let output = fork.contract(USDC);
    let pair: Address = fork.invoke(&fork.contract(ROUTER), "router_pair_for", (&input, &output));
    let token0: Address = fork.invoke(&pair, "token_0", ());
    let reserves: (i128, i128) = fork.invoke(&pair, "get_reserves", ());
    let input_index = u32::from(token0 != input);
    let leg_amount = if input_index == 0 {
        reserves.0
    } else {
        reserves.1
    } / 100
        / 1000
        * 1000;
    assert!(leg_amount > 1000);
    let user = fork.local_account("lumagg-soroswap-user");
    fork.fund_local_account(&user, i64::try_from(2 * leg_amount + 100_000_000).unwrap());
    fork.invoke::<()>(&output, "trust", (&user,));
    fork.invoke::<()>(&output, "set_authorized", (&user, true));
    let admin = fork.local_account("lumagg-local-admin");
    let aggregator = fork.deploy(aggregator::WASM, (&admin,));
    Setup {
        user,
        aggregator,
        pair,
        input,
        output,
        input_index,
        leg_amount,
    }
}

fn state(fork: &ScenarioFork<'_>, p: &Setup) -> State {
    let a = TokenClient::new(fork.env(), &p.input);
    let b = TokenClient::new(fork.env(), &p.output);
    State {
        balances: [
            a.balance(&p.user),
            b.balance(&p.user),
            a.balance(&p.aggregator),
            b.balance(&p.aggregator),
            a.balance(&p.pair),
            b.balance(&p.pair),
        ],
        reserves: fork.invoke(&p.pair, "get_reserves", ()),
    }
}

fn routes(fork: &ScenarioFork<'_>, p: &Setup, legs: u32) -> Vec<aggregator::SubRoute> {
    let mut routes = Vec::new(fork.env());
    for _ in 0..legs {
        routes.push_back(aggregator::SubRoute {
            amount_in: p.leg_amount,
            steps: vec![
                fork.env(),
                aggregator::SwapStep {
                    dex_id: p.pair.clone(),
                    dex_type: aggregator::DexType::SoroswapPair,
                    token_in: p.input.clone(),
                    token_out: p.output.clone(),
                    in_idx: p.input_index,
                    out_idx: 1 - p.input_index,
                },
            ],
        });
    }
    routes
}

// Use the real router's quote implementation, advancing reserve arguments
// between legs. Inputs are multiples of 1000, avoiding a fractional fee.
fn quote(fork: &ScenarioFork<'_>, p: &Setup, legs: u32) -> i128 {
    let (r0, r1): (i128, i128) = fork.invoke(&p.pair, "get_reserves", ());
    let (mut reserve_in, mut reserve_out) = if p.input_index == 0 {
        (r0, r1)
    } else {
        (r1, r0)
    };
    let router = fork.contract(ROUTER);
    let mut total = 0;
    for _ in 0..legs {
        let output: i128 = fork.invoke(
            &router,
            "router_get_amount_out",
            (p.leg_amount, reserve_in, reserve_out),
        );
        reserve_in += p.leg_amount;
        reserve_out -= output;
        total += output;
    }
    total
}

fn assert_settled(fork: &ScenarioFork<'_>, p: &Setup, before: &State, output: i128) {
    let after = state(fork, p);
    assert_eq!(
        after.balances,
        [
            before.balances[0] - 2 * p.leg_amount,
            before.balances[1] + output,
            0,
            0,
            before.balances[4] + 2 * p.leg_amount,
            before.balances[5] - output
        ]
    );
    let reserves = if p.input_index == 0 {
        (after.balances[4], after.balances[5])
    } else {
        (after.balances[5], after.balances[4])
    };
    assert_eq!(after.reserves, reserves);
}

#[test]
fn split_legs_use_updated_reserves_of_the_same_real_pair() {
    runner("same-pair-split")
        .run(|fork| {
            let p = setup(fork);
            let before = state(fork, &p);
            assert_eq!(&before.balances[2..4], &[0, 0]);
            let isolated_leg = quote(fork, &p, 1);
            assert_eq!(state(fork, &p), before);
            let output: i128 = fork.invoke(
                &p.aggregator,
                "swap",
                (&p.user, &p.input, &p.output, routes(fork, &p, 2), 1_i128),
            );
            // A constant-reserve mock pays twice the isolated quote. The real pair
            // makes the second same-direction leg less favorable after the first.
            assert!(output > isolated_leg);
            assert!(output < 2 * isolated_leg);
            assert_settled(fork, &p, &before, output);
            println!(
                "isolated={isolated_leg}, split={output}, input_per_leg={}",
                p.leg_amount
            );
        })
        .unwrap();
}

#[test]
fn aggregate_minimum_is_inclusive_and_rejection_preserves_pool_state() {
    runner("minimum-rollback")
        .run(|fork| {
            let p = setup(fork);
            let before = state(fork, &p);
            let expected = quote(fork, &p, 2);
            assert_eq!(state(fork, &p), before);
            let failure = fork
                .try_invoke::<i128>(
                    &p.aggregator,
                    "swap",
                    (
                        &p.user,
                        &p.input,
                        &p.output,
                        routes(fork, &p, 2),
                        expected + 1,
                    ),
                )
                .unwrap_err();
            assert_eq!(failure.error, Some(ScError::Contract(8)));
            assert_eq!(state(fork, &p), before);
            let output: i128 = fork.invoke(
                &p.aggregator,
                "swap",
                (&p.user, &p.input, &p.output, routes(fork, &p, 2), expected),
            );
            assert_eq!(output, expected);
            assert_settled(fork, &p, &before, output);
        })
        .unwrap();
}
