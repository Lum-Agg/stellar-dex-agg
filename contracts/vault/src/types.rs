use {
    lumagg_contract_types::SubRoute,
    soroban_sdk::{contractclient, Address, Env, Vec},
};

#[contractclient(name = "AggregatorContractClient")]
pub trait AggregatorContract {
    fn round_trip_swap(
        env: Env,
        user: Address,
        base_token: Address,
        bridge_token: Address,
        amount_in: i128,
        leg_out: Vec<SubRoute>,
        leg_back: Vec<SubRoute>,
        min_amount_out: i128,
    ) -> i128;
}
