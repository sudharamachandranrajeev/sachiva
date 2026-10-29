//! The confidentiality checker. Sachiva keeps references only; this looks for
//! text that may identify a patient before it is saved. It warns, it never
//! blocks or sends anything, and it can miss things: it is a safety net.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    HealthNumber,
    Phone,
    BirthDate,
    Postcode,
    Email,
    Name,
    Address,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub kind: Kind,
    /// Byte offsets into the checked text.
    pub start: usize,
    pub end: usize,
    pub text: String,
    /// Plain words for the user.
    pub message: String,
}

fn re(s: &str) -> Regex {
    Regex::new(s).expect("checker pattern")
}

static HEALTH: LazyLock<Regex> = LazyLock::new(|| re(r"\b\d{3}[ -]?\d{3}[ -]?\d{4}\b"));
static PHONE: LazyLock<Regex> = LazyLock::new(|| re(r"(?:\+44[\s-]?\(?0?\)?|\b0)\d(?:[\s-]?\d){8,9}\b"));
static DATE: LazyLock<Regex> = LazyLock::new(|| re(r"\b(\d{1,2})[/.-](\d{1,2})[/.-](\d{4}|\d{2})\b"));
static BIRTH_WORD: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(d\.?o\.?b|born|birth ?date|date of birth)\b"));
static POSTCODE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b[A-Z]{1,2}\d[A-Z\d]?\s*\d[A-Z]{2}\b"));
static EMAIL: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b"));
static TITLED: LazyLock<Regex> = LazyLock::new(|| re(r"\b(?:Mr|Mrs|Ms|Miss|Mx|Master)\.?\s+[A-Z][a-zA-Z'-]+"));
static LABELLED: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b(?:patient|client|service user)(?:'s)?\s*(?:name)?\s*[:=-]\s*[A-Z][a-z'-]+"));
static ADDRESS: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b\d{1,4}[a-z]?\s+(?:[a-z'-]+\s){1,3}(?:road|rd|street|st|avenue|ave|lane|ln|drive|park|gardens|crescent|close|way|terrace|place|court)\b",
    )
});

/// Check `text`. `this_year` decides which dates look like birth dates.
pub fn check(text: &str, this_year: i32) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    let mut add = |kind: Kind, start: usize, end: usize, message: String| {
        if out.iter().any(|f| start < f.end && f.start < end) {
            return;
        }
        out.push(Finding { kind, start, end, text: text[start..end].to_string(), message });
    };

    for m in HEALTH.find_iter(text) {
        let digits: Vec<u32> = m.as_str().chars().filter_map(|c| c.to_digit(10)).collect();
        let msg = if mod11_ok(&digits) {
            "This looks like an NHS or H&C number. Use your reference code instead."
        } else {
            "A 10-digit number, like an NHS or H&C number. Use your reference code instead."
        };
        add(Kind::HealthNumber, m.start(), m.end(), msg.into());
    }
    for m in PHONE.find_iter(text) {
        add(Kind::Phone, m.start(), m.end(), "This looks like a phone number.".into());
    }
    let birth_word = BIRTH_WORD.is_match(text);
    for c in DATE.captures_iter(text) {
        let m = c.get(0).unwrap();
        let y: i32 = c[3].parse().unwrap_or(0);
        let (d, mo): (u32, u32) = (c[1].parse().unwrap_or(0), c[2].parse().unwrap_or(0));
        if !(1..=31).contains(&d) || !(1..=12).contains(&mo) {
            continue;
        }
        let old = if c[3].len() == 4 { y <= this_year - 2 } else { true };
        if birth_word || old {
            add(Kind::BirthDate, m.start(), m.end(), "This date could be a date of birth.".into());
        }
    }
    for m in POSTCODE.find_iter(text) {
        add(Kind::Postcode, m.start(), m.end(), "This looks like a postcode.".into());
    }
    for m in EMAIL.find_iter(text) {
        add(Kind::Email, m.start(), m.end(), "This looks like an e-mail address.".into());
    }
    for m in TITLED.find_iter(text).chain(LABELLED.find_iter(text)) {
        add(Kind::Name, m.start(), m.end(), "This looks like a person's name.".into());
    }
    for m in ADDRESS.find_iter(text) {
        add(Kind::Address, m.start(), m.end(), "This looks like a street address.".into());
    }
    out.sort_by_key(|f| f.start);
    out
}

/// The NHS number check digit (modulus 11). Health and Care numbers in
/// Northern Ireland use the same 10-digit format and check.
fn mod11_ok(d: &[u32]) -> bool {
    if d.len() != 10 {
        return false;
    }
    let sum: u32 = d[..9].iter().zip((2..=10).rev()).map(|(x, w)| x * w).sum();
    match 11 - sum % 11 {
        11 => d[9] == 0,
        10 => false,
        c => d[9] == c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<Kind> {
        check(s, 2026).into_iter().map(|f| f.kind).collect()
    }

    #[test]
    fn health_numbers() {
        // 943 476 5919 is the NHS's published example of a valid number.
        let f = check("Rebook 943 476 5919 next week", 2026);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].kind, Kind::HealthNumber);
        assert_eq!(f[0].text, "943 476 5919");
        assert!(f[0].message.starts_with("This looks like"));
        assert!(check("ref 1234567890", 2026)[0].message.starts_with("A 10-digit"));
    }

    #[test]
    fn phones_postcodes_emails() {
        assert_eq!(kinds("call 07700 900123"), [Kind::Phone]);
        assert_eq!(kinds("call +44 28 9012 3456"), [Kind::Phone]);
        assert_eq!(kinds("lives BT7 1NN"), [Kind::Postcode]);
        assert_eq!(kinds("mail someone@example.com"), [Kind::Email]);
    }

    #[test]
    fn dates_that_could_be_birth_dates() {
        assert_eq!(kinds("born 03/04/1961"), [Kind::BirthDate]);
        assert_eq!(kinds("DOB 3.4.61"), [Kind::BirthDate]);
        assert!(kinds("referral received 12/10/2026").is_empty());
        assert!(kinds("clinic on 31/11/2026").is_empty());
    }

    #[test]
    fn names_and_addresses() {
        assert_eq!(kinds("Ring Mrs Smith about slot"), [Kind::Name]);
        assert_eq!(kinds("Patient: Ann to rebook"), [Kind::Name]);
        assert_eq!(kinds("deliver to 12 Station Road"), [Kind::Address]);
    }

    #[test]
    fn reference_only_text_is_clean() {
        for s in [
            "Offer 14:30 slot to REF-000214, Site B diabetes clinic",
            "Chase PO-0419 dressings delivery",
            "Confirm Dr K clinic list, Thu",
            "Month-end stats for both sites",
        ] {
            assert!(kinds(s).is_empty(), "{s}: {:?}", check(s, 2026));
        }
    }
}
