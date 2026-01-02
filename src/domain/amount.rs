//! Amount type
//!
//! Domain primitive for monetary amounts with business rule validation.
//! All amounts are validated at construction time, ensuring invalid values
//! cannot exist in the system.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Add;
use std::str::FromStr;

/// Maximum allowed balance (1 trillion ATP)
const MAX_AMOUNT: &str = "1000000000000";

/// Maximum decimal places (8)
const MAX_SCALE: u32 = 8;

/// Amount represents a validated monetary value.
/// 
/// # Invariants
/// - Value is always positive (> 0)
/// - Maximum 8 decimal places
/// - Maximum value is 1 trillion ATP
/// 
/// # Example
/// ```
/// use rust_decimal::Decimal;
/// use finance_atp::domain::Amount;
/// 
/// let amount = Amount::new(Decimal::new(100, 0)).unwrap();
/// assert_eq!(amount.value(), Decimal::new(100, 0));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Amount(Decimal);

/// Errors that can occur when creating an Amount
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AmountError {
    #[error("Amount must be positive (got {0})")]
    NotPositive(Decimal),

    #[error("Amount has too many decimal places (max {MAX_SCALE}, got {0})")]
    TooManyDecimals(u32),

    #[error("Amount exceeds maximum allowed value ({MAX_AMOUNT})")]
    Overflow,

    #[error("Invalid amount format: {0}")]
    ParseError(String),
}

impl Amount {
    /// Create a new Amount with validation.
    /// 
    /// # Errors
    /// - `AmountError::NotPositive` if value <= 0
    /// - `AmountError::TooManyDecimals` if more than 8 decimal places
    /// - `AmountError::Overflow` if value > 1 trillion
    pub fn new(value: Decimal) -> Result<Self, AmountError> {
        // Rule 1: Must be positive
        if value <= Decimal::ZERO {
            return Err(AmountError::NotPositive(value));
        }

        // Rule 2: Maximum 8 decimal places
        if value.scale() > MAX_SCALE {
            return Err(AmountError::TooManyDecimals(value.scale()));
        }

        // Rule 3: Maximum 1 trillion ATP
        let max = Decimal::from_str(MAX_AMOUNT).expect("Invalid MAX_AMOUNT constant");
        if value > max {
            return Err(AmountError::Overflow);
        }

        Ok(Self(value))
    }

    /// Create an Amount from an integer (no decimal places).
    pub fn from_integer(value: i64) -> Result<Self, AmountError> {
        Self::new(Decimal::from(value))
    }

    /// Get the underlying Decimal value.
    pub fn value(&self) -> Decimal {
        self.0
    }

    /// Check if this amount can be added to another without overflow.
    pub fn try_add(&self, other: &Amount) -> Result<Amount, AmountError> {
        let sum = self.0 + other.0;
        Amount::new(sum)
    }

    /// Check if this amount is greater than or equal to another.
    pub fn is_sufficient_for(&self, other: &Amount) -> bool {
        self.0 >= other.0
    }

    /// Return zero amount (for internal use)
    /// Note: This is a special case that bypasses validation
    #[allow(dead_code)]
    pub(crate) fn zero() -> Decimal {
        Decimal::ZERO
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.8}", self.0)
    }
}

impl FromStr for Amount {
    type Err = AmountError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let decimal = Decimal::from_str(s)
            .map_err(|e| AmountError::ParseError(e.to_string()))?;
        Amount::new(decimal)
    }
}

impl TryFrom<String> for Amount {
    type Error = AmountError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Amount::from_str(&value)
    }
}

impl From<Amount> for String {
    fn from(amount: Amount) -> Self {
        format!("{:.8}", amount.0)
    }
}

impl Add for Amount {
    type Output = Result<Amount, AmountError>;

    fn add(self, rhs: Self) -> Self::Output {
        self.try_add(&rhs)
    }
}

// Note: We don't implement Sub directly because the result might be <= 0
// Instead, use explicit subtraction with validation

/// Balance represents an account balance (can be zero or positive).
/// Unlike Amount, Balance can be zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Balance(Decimal);

impl Balance {
    /// Create a new balance (zero or positive)
    pub fn new(value: Decimal) -> Result<Self, AmountError> {
        if value < Decimal::ZERO {
            return Err(AmountError::NotPositive(value));
        }

        let max = Decimal::from_str(MAX_AMOUNT).expect("Invalid MAX_AMOUNT constant");
        if value > max {
            return Err(AmountError::Overflow);
        }

        Ok(Self(value))
    }

    /// Create a zero balance
    pub fn zero() -> Self {
        Self(Decimal::ZERO)
    }

    /// Create a balance from a decimal value without validation
    /// WARNING: Only use for system accounts that can have negative balances (e.g., SYSTEM_MINT)
    pub fn from_decimal_unchecked(value: Decimal) -> Self {
        Self(value)
    }

    /// Get the underlying value
    pub fn value(&self) -> Decimal {
        self.0
    }

    /// Check if balance is sufficient for withdrawal
    pub fn is_sufficient_for(&self, amount: &Amount) -> bool {
        self.0 >= amount.value()
    }

    /// Add amount to balance
    pub fn credit(&self, amount: &Amount) -> Result<Balance, AmountError> {
        let new_value = self.0 + amount.value();
        Balance::new(new_value)
    }

    /// Subtract amount from balance
    pub fn debit(&self, amount: &Amount) -> Result<Balance, AmountError> {
        let new_value = self.0 - amount.value();
        Balance::new(new_value)
    }
}

impl fmt::Display for Balance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.8}", self.0)
    }
}

impl Default for Balance {
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // M056: Amount unit tests

    #[test]
    fn test_amount_positive() {
        let amount = Amount::new(Decimal::new(100, 0));
        assert!(amount.is_ok());
        assert_eq!(amount.unwrap().value(), Decimal::new(100, 0));
    }

    #[test]
    fn test_amount_zero_rejected() {
        let amount = Amount::new(Decimal::ZERO);
        assert!(matches!(amount, Err(AmountError::NotPositive(_))));
    }

    #[test]
    fn test_amount_negative_rejected() {
        let amount = Amount::new(Decimal::new(-100, 0));
        assert!(matches!(amount, Err(AmountError::NotPositive(_))));
    }

    #[test]
    fn test_amount_too_many_decimals() {
        // 0.123456789 has 9 decimal places
        let amount = Amount::new(Decimal::new(123456789, 9));
        assert!(matches!(amount, Err(AmountError::TooManyDecimals(9))));
    }

    #[test]
    fn test_amount_max_decimals_ok() {
        // 0.12345678 has 8 decimal places
        let amount = Amount::new(Decimal::new(12345678, 8));
        assert!(amount.is_ok());
    }

    #[test]
    fn test_amount_overflow() {
        // 1 trillion + 1
        let value = Decimal::from_str("1000000000001").unwrap();
        let amount = Amount::new(value);
        assert!(matches!(amount, Err(AmountError::Overflow)));
    }

    #[test]
    fn test_amount_max_value_ok() {
        let value = Decimal::from_str("1000000000000").unwrap();
        let amount = Amount::new(value);
        assert!(amount.is_ok());
    }

    #[test]
    fn test_amount_from_str() {
        let amount: Result<Amount, _> = "123.456".parse();
        assert!(amount.is_ok());
        assert_eq!(amount.unwrap().value(), Decimal::new(123456, 3));
    }

    #[test]
    fn test_amount_try_add() {
        let a = Amount::new(Decimal::new(100, 0)).unwrap();
        let b = Amount::new(Decimal::new(50, 0)).unwrap();
        let sum = a.try_add(&b).unwrap();
        assert_eq!(sum.value(), Decimal::new(150, 0));
    }

    #[test]
    fn test_balance_credit_debit() {
        let balance = Balance::zero();
        let amount = Amount::new(Decimal::new(100, 0)).unwrap();
        
        // Credit
        let balance = balance.credit(&amount).unwrap();
        assert_eq!(balance.value(), Decimal::new(100, 0));
        
        // Debit
        let withdraw = Amount::new(Decimal::new(30, 0)).unwrap();
        let balance = balance.debit(&withdraw).unwrap();
        assert_eq!(balance.value(), Decimal::new(70, 0));
    }

    #[test]
    fn test_balance_insufficient() {
        let balance = Balance::new(Decimal::new(50, 0)).unwrap();
        let amount = Amount::new(Decimal::new(100, 0)).unwrap();
        
        assert!(!balance.is_sufficient_for(&amount));
        
        let result = balance.debit(&amount);
        assert!(matches!(result, Err(AmountError::NotPositive(_))));
    }
}
