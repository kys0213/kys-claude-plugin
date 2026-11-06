//! `<n><unit>` durations (`ms`, `s`, `m`, `h`, `d`) as milliseconds, and the
//! coarse one-unit age label a card shows.

const SEC: u64 = 1_000;
const MIN: u64 = 60 * SEC;
const HOUR: u64 = 60 * MIN;
const DAY: u64 = 24 * HOUR;

pub fn parse_millis(raw: &str) -> Result<u64, String> {
    let split = raw
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| format!("`{raw}` has no unit (ms, s, m, h, d)"))?;
    let (digits, unit) = raw.split_at(split);
    let amount: u64 = digits
        .parse()
        .map_err(|_| format!("`{raw}` does not start with a number"))?;
    let scale = match unit {
        "ms" => 1,
        "s" => SEC,
        "m" => MIN,
        "h" => HOUR,
        "d" => DAY,
        _ => return Err(format!("`{raw}` has an unknown unit (ms, s, m, h, d)")),
    };
    amount
        .checked_mul(scale)
        .ok_or_else(|| format!("`{raw}` is too large"))
}

pub fn age_label(millis: u64) -> String {
    match millis {
        m if m < MIN => format!("{}s", m / SEC),
        m if m < HOUR => format!("{}m", m / MIN),
        m if m < DAY => format!("{}h", m / HOUR),
        m => format!("{}d", m / DAY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_unit() {
        assert_eq!(parse_millis("250ms"), Ok(250));
        assert_eq!(parse_millis("3s"), Ok(3 * SEC));
        assert_eq!(parse_millis("2m"), Ok(2 * MIN));
        assert_eq!(parse_millis("1h"), Ok(HOUR));
        assert_eq!(parse_millis("7d"), Ok(7 * DAY));
    }

    #[test]
    fn rejects_missing_number_unit_or_overflow() {
        assert!(parse_millis("").is_err());
        assert!(parse_millis("10").is_err());
        assert!(parse_millis("m").is_err());
        assert!(parse_millis("2 m").is_err());
        assert!(parse_millis("2w").is_err());
        assert!(parse_millis("-2m").is_err());
        assert!(parse_millis(&format!("{}d", u64::MAX)).is_err());
    }

    #[test]
    fn age_label_floors_to_the_largest_unit() {
        assert_eq!(age_label(0), "0s");
        assert_eq!(age_label(59_999), "59s");
        assert_eq!(age_label(MIN), "1m");
        assert_eq!(age_label(HOUR - 1), "59m");
        assert_eq!(age_label(HOUR), "1h");
        assert_eq!(age_label(DAY + HOUR), "1d");
    }
}
