use rust_decimal::prelude::*;
use rust_decimal::Decimal;

/// Currency scale used by every DECIMAL(15, 2) money column.
pub const MONEY_SCALE: u32 = 2;

pub fn zero() -> Decimal {
    Decimal::ZERO
}

/// Rounds to cents, half-up — the convention finance staff expect on an invoice.
///
/// The result always carries exactly two decimal places. Rounding alone leaves
/// the scale of the input untouched when it is already shorter, so a zero that
/// came back from `SUM(...)` over no rows would otherwise reach the client as
/// `"0"` while every other amount is `"0.00"`.
pub fn round_money(value: Decimal) -> Decimal {
    let mut rounded = value.round_dp_with_strategy(MONEY_SCALE, RoundingStrategy::MidpointAwayFromZero);
    // Only ever pads: rounding above already brought the scale to at most two,
    // so this cannot re-round with `rescale`'s half-even strategy.
    rounded.rescale(MONEY_SCALE);
    rounded
}

/// Restates a transaction amount in the organisation's base currency.
///
/// `rate` is units of base per one unit of the transaction currency, so this is
/// always a multiplication — see `013_multi_currency.sql` for why the rate is
/// stored in that direction.
///
/// Rounded to cents here, once per amount, because the rounded figure is what
/// gets stored and what has to reconcile: summing unrounded products and
/// rounding at the end would disagree with the sum of the stored columns by a
/// cent or two, and that difference is exactly the kind that costs an afternoon
/// to find.
pub fn to_base(amount: Decimal, rate: Decimal) -> Decimal {
    round_money(amount * rate)
}

fn percent(value: Decimal, rate: Decimal) -> Decimal {
    value * rate / Decimal::from(100)
}

/// The three numbers every document line contributes to its parent totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineAmounts {
    /// Quantity x unit price, less discount. Excludes tax.
    pub net: Decimal,
    pub discount: Decimal,
    pub tax: Decimal,
}

impl LineAmounts {
    pub fn gross(&self) -> Decimal {
        round_money(self.net + self.tax)
    }
}

/// `discount_percent` and `tax_rate` are whole percentages (20 means 20%).
pub fn calculate_line(
    quantity: i32,
    unit_price: Decimal,
    discount_percent: Decimal,
    tax_rate: Decimal,
) -> LineAmounts {
    let gross = unit_price * Decimal::from(quantity);
    let discount = round_money(percent(gross, discount_percent));
    let net = round_money(gross - discount);
    let tax = round_money(percent(net, tax_rate));

    LineAmounts { net, discount, tax }
}

/// Document-level totals rolled up from its lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentTotals {
    pub subtotal: Decimal,
    pub tax_amount: Decimal,
    pub total: Decimal,
}

pub fn sum_totals(lines: impl IntoIterator<Item = LineAmounts>) -> DocumentTotals {
    let mut subtotal = Decimal::ZERO;
    let mut tax_amount = Decimal::ZERO;

    for line in lines {
        subtotal += line.net;
        tax_amount += line.tax;
    }

    let subtotal = round_money(subtotal);
    let tax_amount = round_money(tax_amount);

    DocumentTotals { subtotal, tax_amount, total: round_money(subtotal + tax_amount) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn calculates_discount_then_tax() {
        // 3 x 100.00 = 300.00, less 10% = 270.00, plus 20% tax = 54.00
        let line = calculate_line(3, dec!(100.00), dec!(10), dec!(20));
        assert_eq!(line.discount, dec!(30.00));
        assert_eq!(line.net, dec!(270.00));
        assert_eq!(line.tax, dec!(54.00));
        assert_eq!(line.gross(), dec!(324.00));
    }

    #[test]
    fn rounds_half_away_from_zero() {
        // 1 x 0.125 rounds to 0.13, not 0.12 (banker's rounding would give 0.12).
        let line = calculate_line(1, dec!(0.125), dec!(0), dec!(0));
        assert_eq!(line.net, dec!(0.13));
    }

    #[test]
    fn zero_rates_leave_the_amount_untouched() {
        let line = calculate_line(2, dec!(19.99), dec!(0), dec!(0));
        assert_eq!(line.net, dec!(39.98));
        assert_eq!(line.tax, Decimal::ZERO);
    }

    #[test]
    fn totals_are_the_sum_of_their_lines() {
        let totals = sum_totals([
            calculate_line(2, dec!(50.00), dec!(0), dec!(10)),
            calculate_line(1, dec!(25.00), dec!(20), dec!(10)),
        ]);
        assert_eq!(totals.subtotal, dec!(120.00));
        assert_eq!(totals.tax_amount, dec!(12.00));
        assert_eq!(totals.total, dec!(132.00));
    }

    #[test]
    fn money_always_carries_two_decimal_places() {
        // What a `SUM` over no rows hands back.
        assert_eq!(round_money(Decimal::ZERO).to_string(), "0.00");
        assert_eq!(round_money(dec!(7)).to_string(), "7.00");
        assert_eq!(round_money(dec!(7.5)).to_string(), "7.50");
    }

    #[test]
    fn restates_an_amount_at_the_given_rate() {
        // EUR 100.00 at 1.08 is USD 108.00.
        assert_eq!(to_base(dec!(100.00), dec!(1.08)), dec!(108.00));
        // A rate of 1 is the single-currency case and must not disturb anything.
        assert_eq!(to_base(dec!(19.99), Decimal::ONE), dec!(19.99));
    }

    #[test]
    fn a_restated_amount_is_rounded_to_cents() {
        // 33.33 x 1.085 = 36.16305, which is not a payable amount.
        assert_eq!(to_base(dec!(33.33), dec!(1.085)), dec!(36.16));
        // Half-up, consistent with every other money figure.
        assert_eq!(to_base(dec!(10.00), dec!(1.005)), dec!(10.05));
        // Rates with many places do not leak scale into the result.
        assert_eq!(to_base(dec!(1.00), dec!(0.00006512)).to_string(), "0.00");
    }

    #[test]
    fn full_discount_zeroes_the_line() {
        let line = calculate_line(5, dec!(10.00), dec!(100), dec!(20));
        assert_eq!(line.net, Decimal::ZERO);
        assert_eq!(line.tax, Decimal::ZERO);
    }
}
