use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::error::{AppError, AppResult};
use crate::shared::money;

/// A document's currency together with the exchange rate frozen onto it.
///
/// These two travel together everywhere, because neither is usable alone: the
/// code without the rate cannot be restated, and the rate without the code
/// cannot be explained. Producing them as a pair is what stops a document being
/// written with one and not the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCurrency {
    pub code: String,
    /// Units of base currency per one unit of `code`. Exactly 1 for the base
    /// currency itself.
    pub fx_rate: Decimal,
}

impl DocumentCurrency {
    /// The base currency, whose rate against itself is 1 by definition.
    pub fn base(code: impl Into<String>) -> Self {
        Self { code: code.into(), fx_rate: Decimal::ONE }
    }

    pub fn to_base(&self, amount: Decimal) -> Decimal {
        money::to_base(amount, self.fx_rate)
    }

    /// Most money columns are nullable, and a missing amount has a missing base
    /// amount rather than a zero one — a quote with no total has no base total.
    pub fn to_base_opt(&self, amount: Option<Decimal>) -> Option<Decimal> {
        amount.map(|a| self.to_base(a))
    }
}

/// Settles what currency a document is raised in, and at what rate.
///
/// Split into two lookups with the decision on top so that the decision itself
/// is testable without a database: implementors supply the base currency and a
/// rate lookup, and `resolve` is the same logic for all of them.
#[async_trait]
pub trait CurrencyResolver: Send + Sync {
    /// The organisation's configured currency, e.g. `"USD"`.
    async fn base_code(&self) -> AppResult<String>;

    /// The rate in force for `currency` on `on`, or `None` if no rate has been
    /// entered that far back.
    async fn rate_on(&self, currency: &str, on: NaiveDate) -> AppResult<Option<Decimal>>;

    /// `requested` is what the client asked for, if anything; omitting it is the
    /// ordinary case and gets the base currency.
    ///
    /// `on` is the document's own date, not today: an invoice back-dated to
    /// March must be restated at March's rate, and re-raising it tomorrow must
    /// produce the same figure.
    async fn resolve(
        &self,
        requested: Option<&str>,
        on: NaiveDate,
    ) -> AppResult<DocumentCurrency> {
        let base = self.base_code().await?;

        let Some(code) = normalise_requested(requested)? else {
            return Ok(DocumentCurrency::base(base));
        };

        if code == base {
            return Ok(DocumentCurrency::base(base));
        }

        match self.rate_on(&code, on).await? {
            Some(rate) => Ok(DocumentCurrency { code, fx_rate: rate }),
            // Refused rather than defaulted to 1. A missing rate silently
            // treated as parity would book a EUR 10,000 invoice as USD 10,000
            // revenue, and nothing downstream would ever flag it.
            None => Err(AppError::Validation(format!(
                "currency: no exchange rate for {code} on {on}, so an amount in {code} cannot \
                 be restated in {base}. Add a rate effective on or before {on} under Settings \
                 → Exchange Rates, or raise the document in {base}."
            ))),
        }
    }
}

/// Cleans up what the client sent. `None` means "no preference, use the base".
///
/// An empty or blank string is what an untouched form field sends and is not an
/// error; a malformed code is.
pub fn normalise_requested(requested: Option<&str>) -> AppResult<Option<String>> {
    let Some(requested) = requested else {
        return Ok(None);
    };

    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if trimmed.len() != 3 || !trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(AppError::Validation(format!(
            "currency: {trimmed:?} is not a three-letter ISO 4217 code, e.g. USD or EUR."
        )));
    }

    Ok(Some(trimmed.to_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn march(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, day).unwrap()
    }

    struct Fake {
        base: &'static str,
        /// (currency, effective_from, rate), newest last.
        rates: Vec<(&'static str, NaiveDate, Decimal)>,
    }

    #[async_trait]
    impl CurrencyResolver for Fake {
        async fn base_code(&self) -> AppResult<String> {
            Ok(self.base.to_string())
        }

        async fn rate_on(&self, currency: &str, on: NaiveDate) -> AppResult<Option<Decimal>> {
            Ok(self
                .rates
                .iter()
                .filter(|(c, from, _)| *c == currency && *from <= on)
                .max_by_key(|(_, from, _)| *from)
                .map(|(_, _, rate)| *rate))
        }
    }

    fn usd_with_eur() -> Fake {
        Fake {
            base: "USD",
            rates: vec![("EUR", march(1), dec!(1.10)), ("EUR", march(10), dec!(1.15))],
        }
    }

    #[tokio::test]
    async fn omitting_a_currency_takes_the_base_at_parity() {
        let resolved = usd_with_eur().resolve(None, march(5)).await.unwrap();
        assert_eq!(resolved.code, "USD");
        assert_eq!(resolved.fx_rate, Decimal::ONE);
    }

    #[tokio::test]
    async fn an_untouched_form_field_is_not_an_error() {
        for blank in ["", "   "] {
            let resolved = usd_with_eur().resolve(Some(blank), march(5)).await.unwrap();
            assert_eq!(resolved.code, "USD");
        }
    }

    #[tokio::test]
    async fn the_base_currency_never_needs_a_rate() {
        // Note there is no USD row in the fake at all: parity is definitional,
        // not looked up.
        let resolved = usd_with_eur().resolve(Some("usd"), march(5)).await.unwrap();
        assert_eq!(resolved.code, "USD");
        assert_eq!(resolved.fx_rate, Decimal::ONE);
    }

    #[tokio::test]
    async fn a_foreign_currency_gets_the_rate_in_force_on_the_document_date() {
        let fx = usd_with_eur();

        // Between the two rows: the earlier rate is still the one in force.
        assert_eq!(fx.resolve(Some("EUR"), march(5)).await.unwrap().fx_rate, dec!(1.10));
        // On the day a new rate starts, it applies.
        assert_eq!(fx.resolve(Some("EUR"), march(10)).await.unwrap().fx_rate, dec!(1.15));
        assert_eq!(fx.resolve(Some("EUR"), march(20)).await.unwrap().fx_rate, dec!(1.15));
    }

    #[tokio::test]
    async fn a_rate_entered_later_does_not_reach_back() {
        // The document predates every rate on file, so it cannot be restated —
        // rather than borrowing the nearest one from the future.
        let error = usd_with_eur().resolve(Some("EUR"), march(1).pred_opt().unwrap()).await;
        assert!(error.is_err());
    }

    #[tokio::test]
    async fn an_unknown_currency_is_refused_rather_than_assumed_to_be_parity() {
        let error = usd_with_eur().resolve(Some("JPY"), march(5)).await.unwrap_err();
        let message = error.to_string();
        // The message has to say what to do about it, not just "no".
        assert!(message.contains("JPY"), "{message}");
        assert!(message.contains("USD"), "{message}");
        assert!(message.contains("Exchange Rates"), "{message}");
    }

    #[tokio::test]
    async fn a_malformed_code_is_rejected() {
        for bad in ["US", "DOLLAR", "U$D", "12"] {
            assert!(
                usd_with_eur().resolve(Some(bad), march(5)).await.is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }

    #[test]
    fn restating_uses_the_frozen_rate() {
        let eur = DocumentCurrency { code: "EUR".into(), fx_rate: dec!(1.10) };
        assert_eq!(eur.to_base(dec!(1000.00)), dec!(1100.00));
        assert_eq!(eur.to_base_opt(None), None);
        assert_eq!(eur.to_base_opt(Some(dec!(50.00))), Some(dec!(55.00)));
    }
}
