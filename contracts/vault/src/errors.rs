use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VaultError {
    InvalidAmount = 1,
    InvalidMinimumOut = 2,
    InvalidAllowanceExpiry = 3,
    UnauthorizedCaller = 4,
}
