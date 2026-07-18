//! Naira money, as integers.
//!
//! Everything is kobo (1/100 of a naira) in a signed 64-bit integer. No floats
//! anywhere near a total: 0.1 + 0.2 is not 0.3, and an event budget that is off
//! by a kobo per line is an argument with a caterer nobody wants to have.

/// Where a vendor stands, derived from what they cost and what they've been
/// paid. Deliberately computed rather than stored, so it can never disagree
/// with the numbers it describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaidStatus {
    /// Nothing has been paid yet.
    Unpaid,
    /// A deposit has gone out, but not the full cost.
    PartPaid,
    /// Settled in full.
    Paid,
    /// Paid more than the agreed cost. Usually a typo or a cost that was
    /// revised down after a deposit — either way the organizer needs to see it
    /// rather than have it rounded into "paid".
    Overpaid,
}

impl PaidStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PaidStatus::Unpaid => "unpaid",
            PaidStatus::PartPaid => "part_paid",
            PaidStatus::Paid => "paid",
            PaidStatus::Overpaid => "overpaid",
        }
    }
}

pub fn paid_status(cost_kobo: i64, paid_kobo: i64) -> PaidStatus {
    if paid_kobo > cost_kobo {
        return PaidStatus::Overpaid;
    }
    if paid_kobo == 0 {
        // A zero-cost vendor with nothing paid is settled, not unpaid: there
        // is nothing owing. Handshake deals and freebies are common.
        return if cost_kobo == 0 {
            PaidStatus::Paid
        } else {
            PaidStatus::Unpaid
        };
    }
    if paid_kobo == cost_kobo {
        PaidStatus::Paid
    } else {
        PaidStatus::PartPaid
    }
}

/// What's still owed. Never negative — an overpayment is surfaced through
/// [`PaidStatus::Overpaid`], not as a negative balance that would quietly
/// cancel out another vendor's debt in a total.
pub fn outstanding_kobo(cost_kobo: i64, paid_kobo: i64) -> i64 {
    (cost_kobo - paid_kobo).max(0)
}

/// Formats kobo as naira for display: 250000 -> "₦2,500.00".
pub fn format_naira(kobo: i64) -> String {
    let negative = kobo < 0;
    let abs = kobo.unsigned_abs();
    let naira = abs / 100;
    let remainder = abs % 100;

    let digits = naira.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }

    format!("{}₦{grouped}.{remainder:02}", if negative { "-" } else { "" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_follows_the_money() {
        assert_eq!(paid_status(500_000, 0), PaidStatus::Unpaid);
        assert_eq!(paid_status(500_000, 200_000), PaidStatus::PartPaid);
        assert_eq!(paid_status(500_000, 500_000), PaidStatus::Paid);
        assert_eq!(paid_status(500_000, 600_000), PaidStatus::Overpaid);
    }

    #[test]
    fn a_free_vendor_is_settled_not_unpaid() {
        // The uncle who is DJing as a gift still belongs on the sheet, and he
        // is not owed anything.
        assert_eq!(paid_status(0, 0), PaidStatus::Paid);
        assert_eq!(outstanding_kobo(0, 0), 0);
    }

    #[test]
    fn a_deposit_that_lands_exactly_on_cost_is_paid() {
        // Guards the boundary between part-paid and paid.
        assert_eq!(paid_status(1, 1), PaidStatus::Paid);
        assert_eq!(paid_status(2, 1), PaidStatus::PartPaid);
    }

    #[test]
    fn an_overpayment_does_not_become_a_negative_debt() {
        // If this returned -100_000, a sheet with one overpaid vendor and one
        // unpaid one would report less outstanding than is really owed.
        assert_eq!(outstanding_kobo(500_000, 600_000), 0);
        assert_eq!(outstanding_kobo(500_000, 200_000), 300_000);
    }

    #[test]
    fn naira_formats_with_thousands_separators() {
        assert_eq!(format_naira(0), "₦0.00");
        assert_eq!(format_naira(50), "₦0.50");
        assert_eq!(format_naira(250_000), "₦2,500.00");
        assert_eq!(format_naira(100_000_000), "₦1,000,000.00");
        assert_eq!(format_naira(123_456_789), "₦1,234,567.89");
    }

    #[test]
    fn big_budgets_do_not_overflow() {
        // A ₦500,000,000 venue in kobo is 50 billion — well past 32 bits, which
        // is why these columns are BIGINT.
        let venue = 50_000_000_000_i64;
        assert_eq!(format_naira(venue), "₦500,000,000.00");
        assert_eq!(outstanding_kobo(venue, 1), venue - 1);
    }
}
