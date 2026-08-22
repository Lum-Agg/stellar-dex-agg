use soroban_sdk::contracterror;

/// Errors emitted by the aggregator's own validation and accounting logic.
/// Errors raised by an external DEX contract cannot be translated here and
/// remain identifiable by that DEX's contract code.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AggregatorError {
    InvalidAmount = 1,
    InvalidMinimumOut = 2,
    EmptyRoutes = 3,
    InvalidRoute = 4,
    DisconnectedRoute = 5,
    InvalidStep = 6,
    ZeroStepOutput = 7,
    OutputBelowMinimum = 8,
    VenueNotRegistered = 9,
    ArithmeticOverflow = 10,
    NotInitialized = 11,
}
