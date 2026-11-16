#![no_std]

use {
    lumagg_contract_types::{dca_schedule_fits, required_min_out, SubRoute},
    soroban_sdk::{
        auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
        contract, contractclient, contractimpl, contracttype, token, Address, BytesN, Env, IntoVal, Symbol, Vec,
    },
};

mod errors;

pub use errors::EscrowError;

#[contractclient(name = "AggregatorContractClient")]
pub trait AggregatorContract {
    fn swap_restricted(
        env: Env,
        user: Address,
        token_in: Address,
        token_out: Address,
        sub_routes: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128;
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Aggregator,
    NextOrderId,
    NextDcaOrderId,
    Order(u64),
    DcaOrder(u64),
    AllowedAsset(Address),
    Keeper,
}

const LEDGERS_PER_DAY: u32 = 17_280;
const MAX_ORDER_LIFETIME: u32 = 30 * LEDGERS_PER_DAY;
const REFUND_GRACE_LEDGERS: u32 = 7 * LEDGERS_PER_DAY;

#[contracttype]
#[derive(Clone, PartialEq, Eq)]
pub enum OrderStatus {
    Open,
    Filled,
    Cancelled,
    Expired,
}

#[contracttype]
#[derive(Clone)]
pub struct LimitOrder {
    pub owner: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in_remaining: i128,
    pub limit_out_per_in_e7: i128,
    pub expires_ledger: u32,
    pub status: OrderStatus,
}

#[contracttype]
#[derive(Clone)]
pub struct DcaOrder {
    pub owner: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in_remaining: i128,
    pub chunk_amount: i128,
    pub interval_ledgers: u32,
    pub next_executable_ledger: u32,
    /// Required positive per-chunk rate floor.
    pub min_out_per_in_e7: i128,
    pub expires_ledger: u32,
    pub status: OrderStatus,
}

#[contract]
pub struct OrderEscrowContract;

fn require_allowed_assets(env: &Env, token_in: &Address, token_out: &Address) {
    for asset in [token_in, token_out] {
        if !env.storage().instance().has(&DataKey::AllowedAsset(asset.clone())) {
            soroban_sdk::panic_with_error!(env, EscrowError::AssetNotAllowed);
        }
    }
}

fn order_ttl(env: &Env, expires_ledger: u32) -> u32 {
    let ttl = expires_ledger - env.ledger().sequence() + REFUND_GRACE_LEDGERS;
    if ttl > env.storage().max_ttl() {
        soroban_sdk::panic_with_error!(env, EscrowError::InsufficientOrderTtl);
    }
    ttl
}

fn extend_order_ttl(env: &Env, key: &DataKey, ttl: u32) {
    env.storage().persistent().extend_ttl(key, ttl, ttl);
    env.storage().instance().extend_ttl(ttl, ttl);
}

fn require_admin(env: &Env) {
    let Some(admin) = env.storage().instance().get::<_, Address>(&DataKey::Admin) else {
        soroban_sdk::panic_with_error!(env, EscrowError::NotInitialized);
    };
    admin.require_auth();
}

fn transfer_exact(env: &Env, asset: &Address, from: &Address, to: &Address, amount: i128) {
    let client = token::Client::new(env, asset);
    let from_before = client.balance(from);
    let to_before = client.balance(to);
    client.transfer(from, to, &amount);
    if from_before.checked_sub(client.balance(from)) != Some(amount)
        || client.balance(to).checked_sub(to_before) != Some(amount)
    {
        soroban_sdk::panic_with_error!(env, EscrowError::UnexpectedTokenBalance);
    }
}

fn settle_swap(
    env: &Env,
    owner: &Address,
    token_in: &Address,
    token_out: &Address,
    amount_in: i128,
    sub_routes: &Vec<SubRoute>,
    min_amount_out: i128,
) -> i128 {
    require_allowed_assets(env, token_in, token_out);
    let aggregator: Address = env
        .storage()
        .instance()
        .get(&DataKey::Aggregator)
        .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, EscrowError::NotInitialized));
    let escrow = env.current_contract_address();
    let input = token::Client::new(env, token_in);
    let output = token::Client::new(env, token_out);
    let input_before = input.balance(&escrow);
    let output_before = output.balance(&escrow);
    authorize_swap_as_current_contract(env, &aggregator, &escrow, token_in, amount_in);
    let reported = AggregatorContractClient::new(env, &aggregator)
        .swap_restricted(&escrow, token_in, token_out, sub_routes, &min_amount_out);
    if input_before.checked_sub(input.balance(&escrow)) != Some(amount_in)
        || output.balance(&escrow).checked_sub(output_before) != Some(reported)
    {
        soroban_sdk::panic_with_error!(env, EscrowError::UnexpectedTokenBalance);
    }
    if reported < min_amount_out {
        soroban_sdk::panic_with_error!(env, EscrowError::MinimumOutBelowLimit);
    }
    transfer_exact(env, token_out, &escrow, owner, reported);
    reported
}

fn authorize_swap_as_current_contract(
    env: &Env,
    aggregator: &Address,
    escrow: &Address,
    token_in: &Address,
    amount_in: i128,
) {
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token_in.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: soroban_sdk::vec![
                    env,
                    escrow.clone().into_val(env),
                    aggregator.clone().into_val(env),
                    amount_in.into_val(env),
                ],
            },
            sub_invocations: soroban_sdk::vec![env],
        }),
    ]);
}

#[contractimpl]
impl OrderEscrowContract {
    pub fn __constructor(env: Env, admin: Address, aggregator: Address, keeper: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Aggregator, &aggregator);
        env.storage().instance().set(&DataKey::Keeper, &keeper);
        env.storage().instance().set(&DataKey::NextOrderId, &0u64);
        env.storage().instance().set(&DataKey::NextDcaOrderId, &0u64);
    }

    /// Upgrade the escrow WASM code. Only the configured admin can call this.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn keeper(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Keeper)
            .unwrap_or_else(|| soroban_sdk::panic_with_error!(&env, EscrowError::NotInitialized))
    }

    pub fn set_keeper(env: Env, new_keeper: Address) {
        require_admin(&env);
        env.storage().instance().set(&DataKey::Keeper, &new_keeper);
    }

    /// Revoking an asset pauses new orders and fills, but never blocks refunds.
    pub fn set_asset(env: Env, asset: Address, allowed: bool) {
        require_admin(&env);
        let key = DataKey::AllowedAsset(asset);
        if allowed {
            env.storage().instance().set(&key, &true);
        } else {
            env.storage().instance().remove(&key);
        }
    }

    pub fn create_limit(
        env: Env,
        owner: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        limit_out_per_in_e7: i128,
        expires_ledger: u32,
    ) -> u64 {
        owner.require_auth();
        if amount_in <= 0 {
            soroban_sdk::panic_with_error!(&env, EscrowError::InvalidAmount);
        }
        if required_min_out(amount_in, limit_out_per_in_e7).is_none() {
            soroban_sdk::panic_with_error!(&env, EscrowError::InvalidLimit);
        }
        if token_in == token_out {
            soroban_sdk::panic_with_error!(&env, EscrowError::SameToken);
        }
        if expires_ledger <= env.ledger().sequence() {
            soroban_sdk::panic_with_error!(&env, EscrowError::ExpirationInPast);
        }
        if expires_ledger > env.ledger().sequence().saturating_add(MAX_ORDER_LIFETIME) {
            soroban_sdk::panic_with_error!(&env, EscrowError::ExpirationTooFar);
        }
        require_allowed_assets(&env, &token_in, &token_out);
        let ttl = order_ttl(&env, expires_ledger);

        let order_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextOrderId)
            .unwrap_or_else(|| soroban_sdk::panic_with_error!(&env, EscrowError::NotInitialized));
        let escrow = env.current_contract_address();
        transfer_exact(&env, &token_in, &owner, &escrow, amount_in);

        let order = LimitOrder {
            owner,
            token_in,
            token_out,
            amount_in_remaining: amount_in,
            limit_out_per_in_e7,
            expires_ledger,
            status: OrderStatus::Open,
        };
        let key = DataKey::Order(order_id);
        env.storage().persistent().set(&key, &order);
        extend_order_ttl(&env, &key, ttl);
        env.storage().instance().set(
            &DataKey::NextOrderId,
            &order_id
                .checked_add(1)
                .unwrap_or_else(|| soroban_sdk::panic_with_error!(&env, EscrowError::ArithmeticOverflow)),
        );
        env.events().publish(
            (Symbol::new(&env, "order_created"), order_id),
            (
                order.owner.clone(),
                order.token_in.clone(),
                order.token_out.clone(),
                amount_in,
                limit_out_per_in_e7,
                expires_ledger,
            ),
        );
        order_id
    }

    pub fn cancel(env: Env, order_id: u64) {
        let key = DataKey::Order(order_id);
        let Some(mut order) = env.storage().persistent().get::<_, LimitOrder>(&key) else {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotFound);
        };
        order.owner.require_auth();
        if order.status != OrderStatus::Open {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotOpen);
        }

        let escrow = env.current_contract_address();
        let refunded_amount = order.amount_in_remaining;
        transfer_exact(&env, &order.token_in, &escrow, &order.owner, refunded_amount);
        order.amount_in_remaining = 0;
        order.status = OrderStatus::Cancelled;
        env.storage().persistent().set(&key, &order);
        env.events().publish(
            (Symbol::new(&env, "order_cancelled"), order_id),
            (order.owner.clone(), refunded_amount),
        );
    }

    pub fn reclaim_expired(env: Env, order_id: u64) {
        let key = DataKey::Order(order_id);
        let Some(mut order) = env.storage().persistent().get::<_, LimitOrder>(&key) else {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotFound);
        };
        if order.status != OrderStatus::Open {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotOpen);
        }
        if env.ledger().sequence() < order.expires_ledger {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotExpired);
        }

        let escrow = env.current_contract_address();
        let refunded_amount = order.amount_in_remaining;
        transfer_exact(&env, &order.token_in, &escrow, &order.owner, refunded_amount);
        order.amount_in_remaining = 0;
        order.status = OrderStatus::Expired;
        env.storage().persistent().set(&key, &order);
        env.events().publish(
            (Symbol::new(&env, "order_expired"), order_id),
            (order.owner.clone(), refunded_amount),
        );
    }

    pub fn fill(env: Env, order_id: u64, amount_in: i128, sub_routes: Vec<SubRoute>, min_amount_out: i128) -> i128 {
        Self::keeper(env.clone()).require_auth();
        let key = DataKey::Order(order_id);
        let Some(mut order) = env.storage().persistent().get::<_, LimitOrder>(&key) else {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotFound);
        };
        if order.status != OrderStatus::Open {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotOpen);
        }
        if env.ledger().sequence() >= order.expires_ledger {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderExpired);
        }
        if amount_in <= 0 {
            soroban_sdk::panic_with_error!(&env, EscrowError::InvalidAmount);
        }
        if amount_in > order.amount_in_remaining {
            soroban_sdk::panic_with_error!(&env, EscrowError::AmountExceedsRemaining);
        }

        let mut routed_amount = 0i128;
        for route in sub_routes.iter() {
            routed_amount = routed_amount
                .checked_add(route.amount_in)
                .unwrap_or_else(|| soroban_sdk::panic_with_error!(&env, EscrowError::ArithmeticOverflow));
        }
        if routed_amount != amount_in {
            soroban_sdk::panic_with_error!(&env, EscrowError::InvalidRouteAmount);
        }
        if !required_min_out(amount_in, order.limit_out_per_in_e7).is_some_and(|floor| min_amount_out >= floor) {
            soroban_sdk::panic_with_error!(&env, EscrowError::MinimumOutBelowLimit);
        }

        let amount_out = settle_swap(
            &env,
            &order.owner,
            &order.token_in,
            &order.token_out,
            amount_in,
            &sub_routes,
            min_amount_out,
        );

        order.amount_in_remaining -= amount_in;
        if order.amount_in_remaining == 0 {
            order.status = OrderStatus::Filled;
        }
        env.storage().persistent().set(&key, &order);
        env.events().publish(
            (Symbol::new(&env, "order_filled"), order_id),
            (order.owner, amount_in, amount_out, order.amount_in_remaining),
        );
        amount_out
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_dca(
        env: Env,
        owner: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        chunk_amount: i128,
        interval_ledgers: u32,
        start_ledger: u32,
        min_out_per_in_e7: i128,
        expires_ledger: u32,
    ) -> u64 {
        owner.require_auth();
        let current = env.ledger().sequence();
        if amount_in <= 0 {
            soroban_sdk::panic_with_error!(&env, EscrowError::InvalidAmount);
        }
        if chunk_amount <= 0 || chunk_amount > amount_in {
            soroban_sdk::panic_with_error!(&env, EscrowError::InvalidChunk);
        }
        if interval_ledgers == 0 {
            soroban_sdk::panic_with_error!(&env, EscrowError::InvalidInterval);
        }
        if start_ledger < current {
            soroban_sdk::panic_with_error!(&env, EscrowError::StartLedgerInPast);
        }
        if required_min_out(chunk_amount, min_out_per_in_e7).is_none() {
            soroban_sdk::panic_with_error!(&env, EscrowError::InvalidMinimumRate);
        }
        if token_in == token_out {
            soroban_sdk::panic_with_error!(&env, EscrowError::SameToken);
        }
        if expires_ledger <= start_ledger {
            soroban_sdk::panic_with_error!(&env, EscrowError::ExpirationInPast);
        }
        if !dca_schedule_fits(amount_in, chunk_amount, interval_ledgers, start_ledger, expires_ledger) {
            soroban_sdk::panic_with_error!(&env, EscrowError::ScheduleExceedsExpiry);
        }
        if expires_ledger > current.saturating_add(MAX_ORDER_LIFETIME) {
            soroban_sdk::panic_with_error!(&env, EscrowError::ExpirationTooFar);
        }
        require_allowed_assets(&env, &token_in, &token_out);
        let ttl = order_ttl(&env, expires_ledger);

        let order_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextDcaOrderId)
            .unwrap_or_else(|| soroban_sdk::panic_with_error!(&env, EscrowError::NotInitialized));
        let escrow = env.current_contract_address();
        transfer_exact(&env, &token_in, &owner, &escrow, amount_in);

        let order = DcaOrder {
            owner,
            token_in,
            token_out,
            amount_in_remaining: amount_in,
            chunk_amount,
            interval_ledgers,
            next_executable_ledger: start_ledger,
            min_out_per_in_e7,
            expires_ledger,
            status: OrderStatus::Open,
        };
        let key = DataKey::DcaOrder(order_id);
        env.storage().persistent().set(&key, &order);
        extend_order_ttl(&env, &key, ttl);
        env.storage().instance().set(
            &DataKey::NextDcaOrderId,
            &order_id
                .checked_add(1)
                .unwrap_or_else(|| soroban_sdk::panic_with_error!(&env, EscrowError::ArithmeticOverflow)),
        );
        env.events().publish(
            (Symbol::new(&env, "dca_created"), order_id),
            (
                order.owner.clone(),
                order.token_in.clone(),
                order.token_out.clone(),
                amount_in,
                chunk_amount,
                interval_ledgers,
                start_ledger,
                min_out_per_in_e7,
                expires_ledger,
            ),
        );
        order_id
    }

    pub fn fill_dca(env: Env, order_id: u64, sub_routes: Vec<SubRoute>, min_amount_out: i128) -> i128 {
        Self::keeper(env.clone()).require_auth();
        let key = DataKey::DcaOrder(order_id);
        let Some(mut order) = env.storage().persistent().get::<_, DcaOrder>(&key) else {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotFound);
        };
        let current = env.ledger().sequence();
        if order.status != OrderStatus::Open {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotOpen);
        }
        if current >= order.expires_ledger {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderExpired);
        }
        if current < order.next_executable_ledger {
            soroban_sdk::panic_with_error!(&env, EscrowError::ChunkNotDue);
        }

        let amount_in = if order.amount_in_remaining < order.chunk_amount {
            order.amount_in_remaining
        } else {
            order.chunk_amount
        };
        let mut routed_amount = 0i128;
        for route in sub_routes.iter() {
            routed_amount = routed_amount
                .checked_add(route.amount_in)
                .unwrap_or_else(|| soroban_sdk::panic_with_error!(&env, EscrowError::ArithmeticOverflow));
        }
        if routed_amount != amount_in {
            soroban_sdk::panic_with_error!(&env, EscrowError::InvalidRouteAmount);
        }
        if !required_min_out(amount_in, order.min_out_per_in_e7).is_some_and(|floor| min_amount_out >= floor) {
            soroban_sdk::panic_with_error!(&env, EscrowError::MinimumOutBelowLimit);
        }

        let amount_out = settle_swap(
            &env,
            &order.owner,
            &order.token_in,
            &order.token_out,
            amount_in,
            &sub_routes,
            min_amount_out,
        );

        order.amount_in_remaining -= amount_in;
        if order.amount_in_remaining == 0 {
            order.status = OrderStatus::Filled;
        } else {
            order.next_executable_ledger = current.saturating_add(order.interval_ledgers);
        }
        env.storage().persistent().set(&key, &order);
        env.events().publish(
            (Symbol::new(&env, "dca_filled"), order_id),
            (
                order.owner,
                amount_in,
                amount_out,
                order.amount_in_remaining,
                order.next_executable_ledger,
            ),
        );
        amount_out
    }

    pub fn cancel_dca(env: Env, order_id: u64) {
        let key = DataKey::DcaOrder(order_id);
        let Some(mut order) = env.storage().persistent().get::<_, DcaOrder>(&key) else {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotFound);
        };
        order.owner.require_auth();
        if order.status != OrderStatus::Open {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotOpen);
        }
        let refunded_amount = order.amount_in_remaining;
        transfer_exact(
            &env,
            &order.token_in,
            &env.current_contract_address(),
            &order.owner,
            refunded_amount,
        );
        order.amount_in_remaining = 0;
        order.status = OrderStatus::Cancelled;
        env.storage().persistent().set(&key, &order);
        env.events().publish(
            (Symbol::new(&env, "dca_cancelled"), order_id),
            (order.owner, refunded_amount),
        );
    }

    pub fn reclaim_expired_dca(env: Env, order_id: u64) {
        let key = DataKey::DcaOrder(order_id);
        let Some(mut order) = env.storage().persistent().get::<_, DcaOrder>(&key) else {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotFound);
        };
        if order.status != OrderStatus::Open {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotOpen);
        }
        if env.ledger().sequence() < order.expires_ledger {
            soroban_sdk::panic_with_error!(&env, EscrowError::OrderNotExpired);
        }
        let refunded_amount = order.amount_in_remaining;
        transfer_exact(
            &env,
            &order.token_in,
            &env.current_contract_address(),
            &order.owner,
            refunded_amount,
        );
        order.amount_in_remaining = 0;
        order.status = OrderStatus::Expired;
        env.storage().persistent().set(&key, &order);
        env.events().publish(
            (Symbol::new(&env, "dca_expired"), order_id),
            (order.owner, refunded_amount),
        );
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use {
        super::{
            required_min_out, DataKey, DcaOrder, LimitOrder, OrderEscrowContract, OrderEscrowContractClient,
            OrderStatus, MAX_ORDER_LIFETIME,
        },
        aggregator_contract::AggregatorContract,
        lumagg_contract_types::{DexType, SubRoute, SwapStep},
        soroban_sdk::{
            testutils::{Address as _, EnvTestConfig, Events as _, Ledger, LedgerInfo, MockAuth, MockAuthInvoke},
            token, vec, Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
        },
    };

    fn gen_addr(env: &Env) -> Address {
        Address::generate(env)
    }

    fn create_token(env: &Env) -> (Address, token::StellarAssetClient<'static>) {
        let admin = gen_addr(env);
        let address = env.register_stellar_asset_contract_v2(admin).address();
        let sac = token::StellarAssetClient::new(env, &address);
        (address, sac)
    }

    fn create_approved_token(env: &Env, escrow: &OrderEscrowContractClient) -> (Address, token::StellarAssetClient<'static>) {
        let (address, sac) = create_token(env);
        escrow.set_asset(&address, &true);
        (address, sac)
    }

    fn test_env() -> Env {
        Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        })
    }

    mod aq_mock {
        use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

        #[contract]
        pub struct AqPool;

        #[contracttype]
        enum AqKey {
            TokenA,
            TokenB,
        }

        #[contractimpl]
        impl AqPool {
            pub fn init(env: Env, token_a: Address, token_b: Address) {
                env.storage().instance().set(&AqKey::TokenA, &token_a);
                env.storage().instance().set(&AqKey::TokenB, &token_b);
            }

            pub fn swap(env: Env, user: Address, in_idx: u32, out_idx: u32, amount_in: u128, _min: u128) -> u128 {
                user.require_auth();
                let token_a: Address = env.storage().instance().get(&AqKey::TokenA).unwrap();
                let token_b: Address = env.storage().instance().get(&AqKey::TokenB).unwrap();
                let token_in = if in_idx == 0 { &token_a } else { &token_b };
                let token_out = if out_idx == 0 { &token_a } else { &token_b };
                let pool = env.current_contract_address();
                token::Client::new(&env, token_in).transfer(&user, &pool, &(amount_in as i128));
                token::Client::new(&env, token_out).transfer(&pool, &user, &(amount_in as i128));
                amount_in
            }
        }
    }

    fn mock_fill_auth(
        env: &Env,
        escrow: &OrderEscrowContractClient,
        signer: &Address,
        name: &str,
        args: impl IntoVal<Env, Vec<Val>>,
    ) {
        env.mock_auths(&[MockAuth {
            address: signer,
            invoke: &MockAuthInvoke {
                contract: &escrow.address,
                fn_name: name,
                args: args.into_val(env),
                sub_invokes: &[],
            },
        }]);
    }

    #[test]
    fn min_out_scales_with_fill_size() {
        // A limit of 2e7 means two output stroops per input stroop.
        assert_eq!(required_min_out(5_000_000, 20_000_000), Some(10_000_000));
        assert_eq!(required_min_out(3, 5_000_000), Some(2));
    }

    #[test]
    fn min_out_zero_and_overflow_guards() {
        assert_eq!(required_min_out(0, 20_000_000), None);
        assert_eq!(required_min_out(i128::MAX, 10_000_000), Some(i128::MAX));
        assert_eq!(required_min_out(i128::MAX, 10_000_001), None);
    }

    fn setup_escrow<'a>(env: &'a Env) -> (Address, OrderEscrowContractClient<'a>) {
        let escrow_id = env.register(OrderEscrowContract, (gen_addr(env), gen_addr(env), gen_addr(env)));
        let escrow = OrderEscrowContractClient::new(env, &escrow_id);
        (escrow_id, escrow)
    }

    type FillFixture<'a> = (
        Address,
        OrderEscrowContractClient<'a>,
        Address,
        Address,
        token::StellarAssetClient<'a>,
        Address,
    );

    fn setup_fill(env: &Env) -> FillFixture<'_> {
        setup_fill_contracts(env, None)
    }

    fn setup_fill_contracts<'a>(env: &'a Env, wasm: Option<(&[u8], &[u8])>) -> FillFixture<'a> {
        if wasm.is_some() {
            // The host runs WASM constructors below the create-contract frame.
            env.mock_all_auths_allowing_non_root_auth();
        }
        let owner = gen_addr(env);
        let aggregator_admin = gen_addr(env);
        let aggregator_id = match wasm {
            Some((aggregator, _)) => env.register(aggregator, (aggregator_admin,)),
            None => env.register(AggregatorContract, (aggregator_admin,)),
        };
        let aggregator = aggregator_contract::AggregatorContractClient::new(env, &aggregator_id);

        let args = (gen_addr(env), aggregator_id, gen_addr(env));
        let escrow_id = match wasm {
            Some((_, escrow)) => env.register(escrow, args),
            None => env.register(OrderEscrowContract, args),
        };
        if wasm.is_some() {
            env.mock_all_auths();
        }
        let escrow = OrderEscrowContractClient::new(env, &escrow_id);

        let (token_in, token_in_sac) = create_approved_token(env, &escrow);
        let (token_out, token_out_sac) = create_approved_token(env, &escrow);
        token_in_sac.mint(&owner, &10_000);

        let pool_id = env.register(aq_mock::AqPool, ());
        aq_mock::AqPoolClient::new(env, &pool_id).init(&token_in, &token_out);
        token_out_sac.mint(&pool_id, &10_000);
        aggregator.set_venue(&DexType::Aquarius, &pool_id, &true);

        (owner, escrow, token_in, token_out, token_in_sac, pool_id)
    }

    #[test]
    #[ignore = "run scripts/limit-dca-wasm-smoke.sh to build and supply the WASM artifacts"]
    fn wasm_limit_dca_lifecycle() {
        let read_wasm = |key| std::fs::read(std::env::var_os(key).expect("missing WASM path")).expect("read WASM");
        let aggregator = read_wasm("LUMAGG_AGGREGATOR_WASM");
        let escrow_wasm = read_wasm("LUMAGG_ESCROW_WASM");
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, pool_id) =
            setup_fill_contracts(&env, Some((&aggregator, &escrow_wasm)));
        let limit = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);
        let dca = escrow.create_dca(&owner, &token_in, &token_out, &5_000, &2_000, &10, &10, &10_000_000, &100);

        env.ledger().set_sequence_number(10);
        let routes = route(&env, &pool_id, &token_in, &token_out, 2_000);
        let keeper = escrow.keeper();
        env.set_auths(&[]);
        assert!(escrow.try_fill(&limit, &2_000, &routes, &2_000).is_err());
        assert!(escrow.try_fill_dca(&dca, &routes, &2_000).is_err());

        mock_fill_auth(&env, &escrow, &keeper, "fill", (limit, 2_000i128, routes.clone(), 2_001i128));
        assert!(escrow.try_fill(&limit, &2_000, &routes, &2_001).is_err());
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow.address), 10_000);
        assert_eq!(token::Client::new(&env, &token_in).balance(&pool_id), 0);
        assert_eq!(token::Client::new(&env, &token_out).balance(&pool_id), 10_000);
        assert_eq!(order(&env, &escrow.address, limit).amount_in_remaining, 5_000);

        mock_fill_auth(&env, &escrow, &keeper, "fill", (limit, 2_000i128, routes.clone(), 2_000i128));
        assert_eq!(escrow.fill(&limit, &2_000, &routes, &2_000), 2_000);
        mock_fill_auth(&env, &escrow, &keeper, "fill_dca", (dca, routes.clone(), 2_000i128));
        assert_eq!(escrow.fill_dca(&dca, &routes, &2_000), 2_000);

        env.set_auths(&[]);
        env.ledger().set_sequence_number(100);
        escrow.reclaim_expired(&limit);
        escrow.reclaim_expired_dca(&dca);
        let input = token::Client::new(&env, &token_in);
        assert_eq!(input.balance(&owner), 6_000);
        assert_eq!(input.balance(&escrow.address), 0);
        assert_eq!(token::Client::new(&env, &token_out).balance(&owner), 4_000);
    }

    fn route(env: &Env, pool_id: &Address, token_in: &Address, token_out: &Address, amount_in: i128) -> Vec<SubRoute> {
        vec![
            env,
            SubRoute {
                amount_in,
                steps: vec![
                    env,
                    SwapStep {
                        dex_id: pool_id.clone(),
                        dex_type: DexType::Aquarius,
                        token_in: token_in.clone(),
                        token_out: token_out.clone(),
                        in_idx: 0,
                        out_idx: 1,
                    },
                ],
            },
        ]
    }

    fn order(env: &Env, escrow: &Address, order_id: u64) -> LimitOrder {
        env.as_contract(escrow, || {
            env.storage()
                .persistent()
                .get::<_, LimitOrder>(&DataKey::Order(order_id))
                .unwrap()
        })
    }

    fn dca_order(env: &Env, escrow: &Address, order_id: u64) -> DcaOrder {
        env.as_contract(escrow, || {
            env.storage()
                .persistent()
                .get::<_, DcaOrder>(&DataKey::DcaOrder(order_id))
                .unwrap()
        })
    }

    fn has_lifecycle_event(env: &Env, escrow_id: &Address, topic_name: &str, order_id: u64) -> bool {
        let expected_topic = Symbol::new(env, topic_name);
        env.events().all().iter().any(|(contract, topics, _data)| {
            if contract != *escrow_id || topics.len() < 2 {
                return false;
            }
            Symbol::try_from_val(env, &topics.get(0).unwrap()) == Ok(expected_topic.clone()) &&
                u64::try_from_val(env, &topics.get(1).unwrap()) == Ok(order_id)
        })
    }

    #[test]
    fn create_limit_emits_order_created_event() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_approved_token(&env, &escrow);
        let (token_out, _) = create_approved_token(&env, &escrow);
        token_in_sac.mint(&owner, &5_000_000);

        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        assert!(has_lifecycle_event(&env, &escrow_id, "order_created", order_id));
    }

    #[test]
    fn create_limit_rejects_excessive_lifetime() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (_, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_approved_token(&env, &escrow);
        let (token_out, _) = create_approved_token(&env, &escrow);
        token_in_sac.mint(&owner, &5_000);
        let expires = env.ledger().sequence().saturating_add(MAX_ORDER_LIFETIME + 1);

        let result = escrow.try_create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &expires);

        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 5_000);
    }

    #[test]
    fn cancel_emits_order_cancelled_event() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_approved_token(&env, &escrow);
        let (token_out, _) = create_approved_token(&env, &escrow);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        escrow.cancel(&order_id);

        assert!(has_lifecycle_event(&env, &escrow_id, "order_cancelled", order_id));
    }

    #[test]
    fn reclaim_expired_emits_order_expired_event() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_approved_token(&env, &escrow);
        let (token_out, _) = create_approved_token(&env, &escrow);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);
        env.ledger().set(LedgerInfo {
            timestamp: 0,
            protocol_version: 22,
            sequence_number: 101,
            network_id: [0; 32],
            base_reserve: 10_000_000,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 4_096,
            max_entry_ttl: 6_312_000,
        });

        escrow.reclaim_expired(&order_id);

        assert!(has_lifecycle_event(&env, &escrow_id, "order_expired", order_id));
    }

    #[test]
    fn create_limit_pulls_token_in() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_approved_token(&env, &escrow);
        let (token_out, _) = create_approved_token(&env, &escrow);
        token_in_sac.mint(&owner, &5_000_000);

        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        assert_eq!(order_id, 0);
        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 0);
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 5_000_000);
    }

    #[test]
    fn owner_can_cancel_and_refund() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_approved_token(&env, &escrow);
        let (token_out, _) = create_approved_token(&env, &escrow);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        escrow.cancel(&order_id);

        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 5_000_000);
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 0);
    }

    #[test]
    fn reclaim_expired_refunds_open_residual() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_approved_token(&env, &escrow);
        let (token_out, _) = create_approved_token(&env, &escrow);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);
        env.ledger().set(LedgerInfo {
            timestamp: 0,
            protocol_version: 22,
            sequence_number: 101,
            network_id: [0; 32],
            base_reserve: 10_000_000,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 4_096,
            max_entry_ttl: 6_312_000,
        });

        escrow.reclaim_expired(&order_id);

        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 5_000_000);
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 0);
        let order = order(&env, &escrow_id, order_id);
        assert_eq!(order.amount_in_remaining, 0);
        assert!(order.status == OrderStatus::Expired);
    }

    #[test]
    fn reclaim_expired_rejects_second_reclaim() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (_, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_approved_token(&env, &escrow);
        let (token_out, _) = create_approved_token(&env, &escrow);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);
        env.ledger().set(LedgerInfo {
            timestamp: 0,
            protocol_version: 22,
            sequence_number: 101,
            network_id: [0; 32],
            base_reserve: 10_000_000,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 4_096,
            max_entry_ttl: 6_312_000,
        });

        escrow.reclaim_expired(&order_id);

        let result = escrow.try_reclaim_expired(&order_id);
        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
    }

    #[test]
    fn reclaim_expired_rejects_before_expiry() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_approved_token(&env, &escrow);
        let (token_out, _) = create_approved_token(&env, &escrow);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        let result = escrow.try_reclaim_expired(&order_id);

        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 5_000_000);
        assert!(order(&env, &escrow_id, order_id).status == OrderStatus::Open);
    }

    #[test]
    fn non_owner_cannot_cancel() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_approved_token(&env, &escrow);
        let (token_out, _) = create_approved_token(&env, &escrow);
        token_in_sac.mint(&owner, &5_000_000);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000_000, &20_000_000, &100);

        // A caller who cannot supply the owner's authorization must not be able
        // to cancel the order.
        env.set_auths(&[]);
        let result = escrow.try_cancel(&order_id);
        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 5_000_000);
    }

    #[test]
    fn fill_executes_when_limit_met() {
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);
        let routes = route(&env, &pool_id, &token_in, &token_out, 5_000);
        mock_fill_auth(&env, &escrow, &escrow.keeper(), "fill", (order_id, 5_000i128, routes.clone(), 5_000i128));
        let out = escrow.fill(&order_id, &5_000, &routes, &5_000);

        assert_eq!(out, 5_000);
        assert_eq!(token::Client::new(&env, &token_out).balance(&owner), 5_000);
        let order = order(&env, &escrow.address, order_id);
        assert_eq!(order.amount_in_remaining, 0);
        assert!(order.status == OrderStatus::Filled);
    }

    #[test]
    fn fill_rejects_when_min_out_below_limit() {
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);

        let result = escrow.try_fill(
            &order_id,
            &5_000,
            &route(&env, &pool_id, &token_in, &token_out, 5_000),
            &4_999,
        );

        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow.address), 5_000);
    }

    #[test]
    fn fill_rejects_unregistered_venue_without_spending_escrow() {
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, _) = setup_fill(&env);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);
        let unregistered_pool = gen_addr(&env);
        let routes = route(&env, &unregistered_pool, &token_in, &token_out, 5_000);
        mock_fill_auth(&env, &escrow, &escrow.keeper(), "fill", (order_id, 5_000i128, routes.clone(), 5_000i128));
        let result = escrow.try_fill(&order_id, &5_000, &routes, &5_000);

        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow.address), 5_000);
        assert_eq!(order(&env, &escrow.address, order_id).amount_in_remaining, 5_000);
    }

    #[test]
    fn fill_dca_rejects_unregistered_venue_without_spending_escrow() {
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, _) = setup_fill(&env);
        let order_id = escrow.create_dca(&owner, &token_in, &token_out, &5_000, &2_000, &10, &10, &9_000_000, &100);
        let unregistered_pool = gen_addr(&env);
        env.ledger().set_sequence_number(10);
        let routes = route(&env, &unregistered_pool, &token_in, &token_out, 2_000);
        mock_fill_auth(&env, &escrow, &escrow.keeper(), "fill_dca", (order_id, routes.clone(), 2_000i128));
        let result = escrow.try_fill_dca(&order_id, &routes, &2_000);

        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow.address), 5_000);
        assert_eq!(dca_order(&env, &escrow.address, order_id).amount_in_remaining, 5_000);
    }

    #[test]
    fn revoked_asset_blocks_fill_but_not_refund() {
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let order_id = escrow.create_dca(&owner, &token_in, &token_out, &5_000, &2_000, &10, &10, &9_000_000, &100);
        escrow.set_asset(&token_in, &false);
        env.ledger().set_sequence_number(10);
        let result = escrow.try_fill_dca(&order_id, &route(&env, &pool_id, &token_in, &token_out, 2_000), &2_000);
        assert_eq!(
            result,
            Err(Ok(soroban_sdk::Error::from_contract_error(crate::EscrowError::AssetNotAllowed as u32)))
        );
        escrow.cancel_dca(&order_id);
        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 10_000);
    }

    #[test]
    fn unapproved_asset_cannot_be_deposited() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (_, escrow) = setup_escrow(&env);
        let (token_in, sac) = create_token(&env);
        let (token_out, _) = create_approved_token(&env, &escrow);
        sac.mint(&owner, &5_000);
        assert_eq!(
            escrow.try_create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100),
            Err(Ok(soroban_sdk::Error::from_contract_error(crate::EscrowError::AssetNotAllowed as u32)))
        );
        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 5_000);
        env.set_auths(&[]);
        assert!(escrow.try_set_asset(&token_in, &true).is_err());
    }

    #[test]
    fn order_ttl_covers_expiration_and_refund_grace() {
        use soroban_sdk::testutils::storage::{Instance, Persistent};

        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, _) = setup_fill(&env);
        let expires = env.ledger().sequence() + MAX_ORDER_LIFETIME;
        let limit_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &expires);
        let dca_id = escrow.create_dca(&owner, &token_in, &token_out, &5_000, &2_000, &10, &10, &9_000_000, &expires);
        let target = MAX_ORDER_LIFETIME + super::REFUND_GRACE_LEDGERS;
        env.as_contract(&escrow.address, || {
            assert!(env.storage().persistent().get_ttl(&DataKey::Order(limit_id)) >= target);
            assert!(env.storage().persistent().get_ttl(&DataKey::DcaOrder(dca_id)) >= target);
            assert!(env.storage().instance().get_ttl() >= target);
        });
    }

    #[test]
    fn fill_rejects_expired() {
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);
        env.ledger().set(LedgerInfo {
            timestamp: 0,
            protocol_version: 22,
            sequence_number: 100,
            network_id: [0; 32],
            base_reserve: 10_000_000,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 4_096,
            max_entry_ttl: 6_312_000,
        });

        let result = escrow.try_fill(
            &order_id,
            &5_000,
            &route(&env, &pool_id, &token_in, &token_out, 5_000),
            &5_000,
        );

        assert!(matches!(result, Ok(Err(_)) | Err(_)), "{result:?}");
    }

    #[test]
    fn partial_fill_reduces_remaining() {
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let order_id = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);
        let routes = route(&env, &pool_id, &token_in, &token_out, 2_000);
        mock_fill_auth(&env, &escrow, &escrow.keeper(), "fill", (order_id, 2_000i128, routes.clone(), 2_000i128));
        assert_eq!(escrow.fill(&order_id, &2_000, &routes, &2_000), 2_000);

        let order = order(&env, &escrow.address, order_id);
        assert_eq!(order.amount_in_remaining, 3_000);
        assert!(order.status == OrderStatus::Open);
    }

    #[test]
    fn only_current_keeper_can_fill_either_order_type() {
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let limit = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);
        let dca = escrow.create_dca(&owner, &token_in, &token_out, &5_000, &2_000, &10, &10, &10_000_000, &100);
        let previous = escrow.keeper();
        let current = gen_addr(&env);
        env.set_auths(&[]);
        assert!(escrow.try_set_keeper(&current).is_err());
        assert_eq!(escrow.keeper(), previous);
        env.mock_all_auths();
        escrow.set_keeper(&current);
        assert_eq!(escrow.keeper(), current);

        env.ledger().set_sequence_number(10);
        let routes = route(&env, &pool_id, &token_in, &token_out, 2_000);
        for unauthorized in [&owner, &previous] {
            mock_fill_auth(&env, &escrow, unauthorized, "fill", (limit, 2_000i128, routes.clone(), 2_000i128));
            assert!(escrow.try_fill(&limit, &2_000, &routes, &2_000).is_err());
            mock_fill_auth(&env, &escrow, unauthorized, "fill_dca", (dca, routes.clone(), 2_000i128));
            assert!(escrow.try_fill_dca(&dca, &routes, &2_000).is_err());
        }
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow.address), 10_000);
        mock_fill_auth(&env, &escrow, &current, "fill", (limit, 2_000i128, routes.clone(), 2_000i128));
        assert_eq!(escrow.fill(&limit, &2_000, &routes, &2_000), 2_000);
        mock_fill_auth(&env, &escrow, &current, "fill_dca", (dca, routes.clone(), 2_000i128));
        assert_eq!(escrow.fill_dca(&dca, &routes, &2_000), 2_000);
        assert_eq!(token::Client::new(&env, &token_out).balance(&owner), 4_000);
    }

    #[test]
    fn dca_executes_one_chunk_per_interval_and_handles_final_remainder() {
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, pool_id) = setup_fill(&env);
        let escrow_id = escrow.address.clone();
        assert_eq!(
            escrow.try_create_dca(&owner, &token_in, &token_out, &5_000, &2_000, &10, &10, &0, &100),
            Err(Ok(soroban_sdk::Error::from_contract_error(crate::EscrowError::InvalidMinimumRate as u32)))
        );
        let order_id = escrow.create_dca(&owner, &token_in, &token_out, &5_000, &2_000, &10, &10, &9_000_000, &100);
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 5_000);
        let keeper = escrow.keeper();
        let routes = route(&env, &pool_id, &token_in, &token_out, 2_000);
        let authorize_chunk = || mock_fill_auth(&env, &escrow, &keeper, "fill_dca", (order_id, routes.clone(), 1_900i128));
        authorize_chunk();

        let early = escrow.try_fill_dca(&order_id, &route(&env, &pool_id, &token_in, &token_out, 2_000), &1_900);
        assert!(matches!(early, Ok(Err(_)) | Err(_)), "{early:?}");

        env.ledger().set_sequence_number(10);
        authorize_chunk();
        assert_eq!(
            escrow.fill_dca(&order_id, &route(&env, &pool_id, &token_in, &token_out, 2_000), &1_900),
            2_000
        );
        let after_first = dca_order(&env, &escrow_id, order_id);
        assert_eq!(after_first.amount_in_remaining, 3_000);
        assert_eq!(after_first.next_executable_ledger, 20);

        authorize_chunk();
        let duplicate = escrow.try_fill_dca(&order_id, &routes, &1_900);
        assert!(matches!(duplicate, Ok(Err(_)) | Err(_)), "{duplicate:?}");

        env.ledger().set_sequence_number(20);
        authorize_chunk();
        escrow.fill_dca(&order_id, &route(&env, &pool_id, &token_in, &token_out, 2_000), &1_900);
        env.ledger().set_sequence_number(30);
        let remainder_routes = route(&env, &pool_id, &token_in, &token_out, 1_000);
        mock_fill_auth(&env, &escrow, &keeper, "fill_dca", (order_id, remainder_routes.clone(), 900i128));
        escrow.fill_dca(&order_id, &remainder_routes, &900);

        let completed = dca_order(&env, &escrow_id, order_id);
        assert_eq!(completed.amount_in_remaining, 0);
        assert!(completed.status == OrderStatus::Filled);
        assert_eq!(token::Client::new(&env, &token_out).balance(&owner), 5_000);
    }

    #[test]
    fn dca_rejects_final_chunk_at_expiry_before_deposit() {
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, _) = setup_fill(&env);
        assert_eq!(
            escrow.try_create_dca(&owner, &token_in, &token_out, &5_000, &2_000, &10, &10, &9_000_000, &30),
            Err(Ok(soroban_sdk::Error::from_contract_error(crate::EscrowError::ScheduleExceedsExpiry as u32)))
        );
        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 10_000);
        escrow.create_dca(&owner, &token_in, &token_out, &5_000, &2_000, &10, &10, &9_000_000, &31);
    }

    #[test]
    fn both_order_types_can_refund_at_expiry_without_owner_auth() {
        let env = test_env();
        env.mock_all_auths();
        let (owner, escrow, token_in, token_out, _, _) = setup_fill(&env);
        let limit = escrow.create_limit(&owner, &token_in, &token_out, &5_000, &10_000_000, &100);
        let dca = escrow.create_dca(&owner, &token_in, &token_out, &5_000, &2_000, &10, &10, &9_000_000, &100);
        env.set_auths(&[]);
        env.ledger().set_sequence_number(99);
        assert!(escrow.try_reclaim_expired(&limit).is_err());
        assert!(escrow.try_reclaim_expired_dca(&dca).is_err());
        env.ledger().set_sequence_number(100);
        escrow.reclaim_expired(&limit);
        escrow.reclaim_expired_dca(&dca);
        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 10_000);
    }

    #[test]
    fn dca_cancel_refunds_unspent_principal() {
        let env = test_env();
        env.mock_all_auths();
        let owner = gen_addr(&env);
        let (escrow_id, escrow) = setup_escrow(&env);
        let (token_in, token_in_sac) = create_approved_token(&env, &escrow);
        let (token_out, _) = create_approved_token(&env, &escrow);
        token_in_sac.mint(&owner, &5_000);

        let order_id = escrow.create_dca(&owner, &token_in, &token_out, &5_000, &1_000, &10, &10, &9_000_000, &100);
        escrow.cancel_dca(&order_id);

        assert_eq!(token::Client::new(&env, &token_in).balance(&owner), 5_000);
        assert_eq!(token::Client::new(&env, &token_in).balance(&escrow_id), 0);
        assert!(dca_order(&env, &escrow_id, order_id).status == OrderStatus::Cancelled);
    }
}
