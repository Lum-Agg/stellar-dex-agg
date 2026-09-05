use soroban_sdk::contracterror;

/// Stable error codes for caller-visible escrow validation failures.
/// Structured errors are easier to diagnose than Soroban's generic trap.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    InvalidAmount = 3,
    InvalidLimit = 4,
    SameToken = 5,
    ExpirationInPast = 6,
    ExpirationTooFar = 7,
    OrderNotFound = 8,
    OrderNotOpen = 9,
    OrderExpired = 10,
    OrderNotExpired = 11,
    AmountExceedsRemaining = 12,
    InvalidRouteAmount = 13,
    MinimumOutBelowLimit = 14,
    InvalidChunk = 15,
    InvalidInterval = 16,
    StartLedgerInPast = 17,
    InvalidMinimumRate = 18,
    ChunkNotDue = 19,
    ArithmeticOverflow = 20,
}
