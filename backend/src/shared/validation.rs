use axum::{
    async_trait,
    extract::{rejection::JsonRejection, FromRequest, Request},
    Json,
};
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::error::AppError;

/// `Json<T>` that runs `validator` before the handler ever sees the body.
///
/// Handlers stay thin: they take `ValidatedJson<CreateFooRequest>` and can assume
/// every `#[validate(..)]` rule on the DTO already passed.
pub struct ValidatedJson<T>(pub T);

#[async_trait]
impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|rejection| AppError::BadRequest(rejection.body_text()))?;

        value
            .validate()
            .map_err(|errors| AppError::Validation(format_errors(&errors)))?;

        Ok(ValidatedJson(value))
    }
}

/// Flattens `validator`'s nested error map into one readable sentence, e.g.
/// `email: invalid email format; password: must be at least 8 characters`.
///
/// Descends into nested structs and lists, so a bad document line reports
/// `lines[2].tax_rate: …` rather than the bare "Validation failed" that
/// `field_errors()` alone produces — it only returns top-level fields, and
/// every rule on a line item lives one level down.
fn format_errors(errors: &validator::ValidationErrors) -> String {
    let mut messages = Vec::new();
    collect(errors, "", &mut messages);

    messages.sort();
    if messages.is_empty() {
        "Validation failed".to_string()
    } else {
        messages.join("; ")
    }
}

fn collect(errors: &validator::ValidationErrors, prefix: &str, out: &mut Vec<String>) {
    use validator::ValidationErrorsKind;

    for (field, kind) in errors.errors() {
        let path = if prefix.is_empty() {
            field.to_string()
        } else {
            format!("{prefix}.{field}")
        };

        match kind {
            ValidationErrorsKind::Field(errs) => {
                let detail = errs
                    .iter()
                    .map(|e| match &e.message {
                        Some(m) => m.to_string(),
                        None => format!("failed `{}` validation", e.code),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push(format!("{path}: {detail}"));
            }
            ValidationErrorsKind::Struct(nested) => collect(nested, &path, out),
            ValidationErrorsKind::List(items) => {
                // Indexed so the client can point at the offending row.
                for (index, nested) in items {
                    collect(nested, &format!("{path}[{index}]"), out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize, Validate)]
    struct Sample {
        #[validate(length(min = 2, message = "too short"))]
        name: String,
        #[validate(range(min = 1))]
        qty: i32,
    }

    #[test]
    fn formats_messages_with_field_names() {
        let sample = Sample { name: "a".into(), qty: 0 };
        let err = sample.validate().unwrap_err();
        let formatted = format_errors(&err);
        assert!(formatted.contains("name: too short"));
        assert!(formatted.contains("qty: failed `range` validation"));
    }

    #[test]
    fn valid_input_produces_no_errors() {
        let sample = Sample { name: "ab".into(), qty: 1 };
        assert!(sample.validate().is_ok());
    }

    #[derive(Debug, serde::Deserialize, Validate)]
    struct Document {
        #[validate]
        lines: Vec<Sample>,
    }

    #[test]
    fn names_the_offending_line_and_field() {
        let document = Document {
            lines: vec![Sample { name: "ok".into(), qty: 1 }, Sample { name: "a".into(), qty: 0 }],
        };

        let formatted = format_errors(&document.validate().unwrap_err());

        // Without descending into the list this would read "Validation failed",
        // leaving the client to guess which of ten lines was wrong.
        assert!(formatted.contains("lines[1].name: too short"), "{formatted}");
        assert!(formatted.contains("lines[1].qty"), "{formatted}");
        assert!(!formatted.contains("lines[0]"), "{formatted}");
    }

    #[test]
    fn a_percentage_is_bounded_at_both_ends() {
        use rust_decimal_macros::dec;

        assert!(validate_percentage(&dec!(0)).is_ok());
        assert!(validate_percentage(&dec!(20)).is_ok());
        assert!(validate_percentage(&dec!(100)).is_ok());
        assert!(validate_percentage(&dec!(100.01)).is_err());
        assert!(validate_percentage(&dec!(-0.01)).is_err());
    }
}

/// Rejects anything outside 0..=100.
///
/// Every rate and discount in this application is a whole percentage — `20`
/// means 20%, on a document line and in `accounting.tax_rates` alike. One
/// definition, referenced from every module, so the convention cannot drift
/// apart again.
pub fn validate_percentage(value: &rust_decimal::Decimal) -> Result<(), validator::ValidationError> {
    use rust_decimal::Decimal;

    if *value >= Decimal::ZERO && *value <= Decimal::from(100) {
        return Ok(());
    }

    let mut error = validator::ValidationError::new("percentage");
    error.message = Some("Must be a percentage between 0 and 100 (20 means 20%)".into());
    Err(error)
}
