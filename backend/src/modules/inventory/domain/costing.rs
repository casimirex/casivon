//! What a unit of stock is worth.
//!
//! Moving weighted average: one running cost per product, nudged toward the
//! purchase price every time more arrives, and left alone when stock goes out.
//! A sale consumes at whatever the average is at that moment.
//!
//! Pure, so the arithmetic that decides what reaches the ledger can be argued
//! with in a unit test rather than through a database and a posting run.

use rust_decimal::prelude::*;
use rust_decimal::Decimal;

/// Decimal places kept on a unit cost.
///
/// Four rather than the usual two because this is a cost *per unit*: a genuine
/// average lands on 3.3333 all the time, and rounding that to pennies per unit
/// drifts badly over a few thousand of them. What reaches the ledger is still
/// rounded to cents by `round_money`; the extra precision lives here.
pub const COST_SCALE: u32 = 4;

/// Rounds a unit cost to the scale the column holds.
pub fn round_cost(value: Decimal) -> Decimal {
    let mut rounded =
        value.round_dp_with_strategy(COST_SCALE, RoundingStrategy::MidpointAwayFromZero);
    rounded.rescale(COST_SCALE);
    rounded
}

/// The new average after `incoming` units arrive at `cost` each.
///
/// `on_hand` and `current` describe the stock *before* the arrival. `current` is
/// `None` for a product that has never been costed, which is ordinary: a product
/// created without a `cost_price` has nothing to average against until its first
/// receipt tells it what it costs.
///
/// The weighting is by quantity, so a large delivery at a new price moves the
/// average further than a small one — which is the whole point of the method,
/// and the reason this cannot be a simple midpoint.
pub fn moving_average(
    on_hand: i32,
    current: Option<Decimal>,
    incoming: i32,
    cost: Decimal,
) -> Decimal {
    let current = current.unwrap_or(Decimal::ZERO);

    // Nothing arriving cannot change what the existing stock cost. Guards a
    // caller that hands us a zero or negative quantity as much as it states the
    // rule.
    if incoming <= 0 {
        return round_cost(current);
    }

    // Stock at or below zero has no value to blend with, so the arrival simply
    // sets the cost. Below zero is the interesting case: it means the books
    // recorded a sale before the receipt that supplied it, and averaging against
    // a negative quantity would produce a cost on the wrong side of zero — a
    // nonsense figure that would then be posted. Taking the incoming cost is the
    // honest answer to "what does this stock cost?" once the history is
    // contradictory.
    if on_hand <= 0 {
        return round_cost(cost);
    }

    let on_hand = Decimal::from(on_hand);
    let incoming_qty = Decimal::from(incoming);

    round_cost((on_hand * current + incoming_qty * cost) / (on_hand + incoming_qty))
}

/// The new average after `removed` units leave at a known `cost` each.
///
/// The inverse of [`moving_average`], and it exists for exactly one caller: a
/// purchase return, where the goods go back to the supplier at the price they
/// arrived at rather than at what the shelf now averages. Removing them at that
/// price without un-blending the average would leave the stock valuation and the
/// Inventory account disagreeing by the difference.
///
/// Ordinary outward movements do *not* use this — a sale consumes at the average
/// and leaves it alone, which is the whole idea of a weighted average.
///
/// ```text
/// 150 @ 4.5000 = 675.00,  return 10 that came in at 5.50
/// -> 140 units worth 620.00,  average 4.4286
/// ```
pub fn average_after_removal(
    on_hand: i32,
    current: Option<Decimal>,
    removed: i32,
    cost: Decimal,
) -> Decimal {
    let current = current.unwrap_or(Decimal::ZERO);

    if removed <= 0 {
        return round_cost(current);
    }

    // Nothing left to carry a cost. Keeping the old average rather than zeroing
    // it means the next arrival blends against a sensible figure instead of
    // treating an empty shelf as free stock — and with no quantity on hand the
    // value is nil either way.
    if removed >= on_hand {
        return round_cost(current);
    }

    let remaining_value = Decimal::from(on_hand) * current - Decimal::from(removed) * cost;

    // A return priced above what the shelf averages can take the remaining value
    // below zero — returning the expensive half of two deliveries, say. Stock
    // cannot be worth less than nothing, and posting a negative unit cost would
    // put a credit balance on an asset.
    if remaining_value <= Decimal::ZERO {
        return Decimal::ZERO;
    }

    round_cost(remaining_value / Decimal::from(on_hand - removed))
}

/// What `quantity` units are worth at `average`, as a ledger amount.
///
/// Rounded to cents here rather than by the caller, because this figure is a
/// journal amount and every journal amount in the system is rounded once, at the
/// point it becomes one.
pub fn extended_cost(quantity: i32, average: Option<Decimal>) -> Decimal {
    let average = average.unwrap_or(Decimal::ZERO);
    crate::shared::money::round_money(Decimal::from(quantity.abs()) * average)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn the_first_receipt_sets_the_cost() {
        assert_eq!(moving_average(0, None, 100, dec!(4.00)), dec!(4.0000));
    }

    #[test]
    fn a_second_receipt_lands_between_the_two_weighted_by_quantity() {
        // 100 @ 4.00 then 50 @ 5.50 -> (400 + 275) / 150 = 4.50
        assert_eq!(moving_average(100, Some(dec!(4.00)), 50, dec!(5.50)), dec!(4.5000));
    }

    /// The point of weighting: the same two prices, different quantities, a
    /// different answer. A midpoint would give 4.75 for both.
    #[test]
    fn a_large_delivery_moves_the_average_further_than_a_small_one() {
        let small = moving_average(100, Some(dec!(4.00)), 10, dec!(5.50));
        let large = moving_average(100, Some(dec!(4.00)), 400, dec!(5.50));

        assert_eq!(small, dec!(4.1364));
        assert_eq!(large, dec!(5.2000));
        assert!(small < large);
    }

    #[test]
    fn a_product_that_had_no_cost_takes_the_one_that_arrives() {
        assert_eq!(moving_average(30, None, 10, dec!(2.50)), dec!(0.6250));
        //          ^ 30 units carried at nothing, so the arrival is diluted
        //            across all 40 rather than simply replacing the cost.
    }

    #[test]
    fn nothing_arriving_leaves_the_average_alone() {
        assert_eq!(moving_average(100, Some(dec!(4.00)), 0, dec!(9.99)), dec!(4.0000));
        assert_eq!(moving_average(100, Some(dec!(4.00)), -5, dec!(9.99)), dec!(4.0000));
    }

    /// Oversold stock: a sale was recorded before the receipt that supplied it.
    /// Averaging against a negative quantity would put the cost on the wrong
    /// side of zero and that figure would then be posted.
    #[test]
    fn negative_stock_does_not_produce_a_negative_cost() {
        let cost = moving_average(-20, Some(dec!(4.00)), 10, dec!(6.00));
        assert_eq!(cost, dec!(6.0000));
        assert!(cost > Decimal::ZERO);
    }

    #[test]
    fn an_empty_shelf_does_not_divide_by_zero() {
        assert_eq!(moving_average(0, Some(dec!(4.00)), 10, dec!(6.00)), dec!(6.0000));
    }

    #[test]
    fn a_repeating_average_keeps_four_places() {
        // (10 × 1.00 + 20 × 5.00) / 30 = 3.6666...
        assert_eq!(moving_average(10, Some(dec!(1.00)), 20, dec!(5.00)), dec!(3.6667));
    }

    // ---- returning goods to a supplier -----------------------------------

    #[test]
    fn removing_stock_un_blends_the_average() {
        // 150 @ 4.50 = 675.00; send back 10 of the delivery that cost 5.50.
        // 675 − 55 = 620 over 140 units.
        assert_eq!(average_after_removal(150, Some(dec!(4.50)), 10, dec!(5.50)), dec!(4.4286));
    }

    /// Returning goods at the price they arrived at leaves the rest carrying
    /// exactly what they cost — which is what keeps the valuation report and the
    /// Inventory account the same number.
    #[test]
    fn removing_at_the_price_they_arrived_at_leaves_the_rest_untouched() {
        // Everything came in at 4.00, so taking some back cannot move the average.
        assert_eq!(average_after_removal(100, Some(dec!(4.00)), 25, dec!(4.00)), dec!(4.0000));
    }

    #[test]
    fn removing_the_cheap_half_raises_what_is_left() {
        // 100 @ 4.00 and 100 @ 6.00 average 5.00. Send back the cheap hundred
        // and the shelf is worth 6.00 a unit, which it is.
        assert_eq!(average_after_removal(200, Some(dec!(5.00)), 100, dec!(4.00)), dec!(6.0000));
    }

    #[test]
    fn removing_nothing_changes_nothing() {
        assert_eq!(average_after_removal(100, Some(dec!(4.00)), 0, dec!(9.99)), dec!(4.0000));
        assert_eq!(average_after_removal(100, Some(dec!(4.00)), -5, dec!(9.99)), dec!(4.0000));
    }

    #[test]
    fn emptying_the_shelf_keeps_the_last_known_cost() {
        // No quantity left, so the value is nil either way; keeping the figure
        // means the next delivery blends against something sensible.
        assert_eq!(average_after_removal(40, Some(dec!(4.00)), 40, dec!(4.00)), dec!(4.0000));
        assert_eq!(average_after_removal(40, Some(dec!(4.00)), 50, dec!(4.00)), dec!(4.0000));
    }

    /// Returning goods worth more than the whole shelf averages out to. Stock
    /// cannot be worth less than nothing, and a negative unit cost would post a
    /// credit balance onto an asset.
    #[test]
    fn a_return_cannot_drive_the_cost_below_zero() {
        assert_eq!(average_after_removal(100, Some(dec!(1.00)), 10, dec!(50.00)), Decimal::ZERO);
    }

    #[test]
    fn extended_cost_is_a_ledger_amount() {
        assert_eq!(extended_cost(60, Some(dec!(4.5000))), dec!(270.00));
        // Rounded once, to cents, because this becomes a journal line.
        assert_eq!(extended_cost(3, Some(dec!(3.3333))), dec!(10.00));
        assert_eq!(extended_cost(7, Some(dec!(1.1111))), dec!(7.78));
    }

    /// Adjustments carry their own sign; the *value* they move is the same
    /// either way, and the debit/credit side is what expresses the direction.
    #[test]
    fn extended_cost_ignores_the_sign_of_the_quantity() {
        assert_eq!(extended_cost(-4, Some(dec!(2.50))), dec!(10.00));
    }

    #[test]
    fn an_uncosted_product_is_worth_nothing_rather_than_failing() {
        assert_eq!(extended_cost(10, None), dec!(0.00));
    }
}
