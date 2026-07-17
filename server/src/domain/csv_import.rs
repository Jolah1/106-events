//! Reading a guest list out of whatever spreadsheet the organizer already has.
//!
//! Nobody builds their guest list in our product first. It exists in Excel or
//! Google Sheets months before we're involved, with columns named whatever the
//! person typing felt like. So this module maps headers by meaning rather than
//! demanding an exact template, and reports problems per row: one bad phone
//! number in row 300 must not cost the organizer the other 299.

use std::borrow::Cow;

use crate::domain::phone;

/// One guest, as read from the file. Sub-event names are still raw strings
/// here; resolving them against the event's parts needs the database.
#[derive(Debug, PartialEq)]
pub struct ImportRow {
    /// 1-based line in the source file, so an error can point the organizer at
    /// the row they can actually see in Excel.
    pub line: u64,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    /// `None` when the file says nothing about this guest's plus-ones — which
    /// is not the same as saying zero. Re-importing a spreadsheet that has no
    /// plus-ones column must leave the numbers the organizer set by hand.
    pub plus_ones: Option<i32>,
    pub dietary: String,
    pub notes: String,
    /// Empty when the file says nothing about which parts the guest attends.
    pub parts: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct RowError {
    pub line: u64,
    pub message: String,
}

#[derive(Debug, PartialEq)]
pub struct ParsedCsv {
    pub rows: Vec<ImportRow>,
    pub errors: Vec<RowError>,
    /// Header names we didn't recognise, echoed back so the organizer can see
    /// that their "Table Number" column was ignored rather than lost.
    pub ignored_columns: Vec<String>,
}

/// Fails the whole file, as opposed to a single row.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum CsvError {
    #[error("the file is empty")]
    Empty,
    #[error("couldn't find a name column — expected a header like \"Name\" or \"Guest\"")]
    NoNameColumn,
    #[error("the file is malformed: {0}")]
    Malformed(String),
    #[error("that's {0} rows; the limit is {MAX_ROWS} per import")]
    TooManyRows(usize),
}

pub const MAX_ROWS: usize = 5_000;

/// What a column means. Everything else is ignored.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Column {
    Name,
    Phone,
    Email,
    PlusOnes,
    Dietary,
    Notes,
    Parts,
}

/// Header spellings seen in real guest lists. Compared against the header
/// reduced to lowercase words, so "Phone Number", "phone_number" and
/// "PHONE NUMBER" are all the same key.
fn classify(header: &str) -> Option<Column> {
    Some(match header {
        "name" | "full name" | "fullname" | "guest" | "guest name" | "invitee" => Column::Name,
        "phone" | "phone number" | "mobile" | "mobile number" | "tel" | "telephone"
        | "number" | "whatsapp" | "whatsapp number" | "msisdn" | "contact" => Column::Phone,
        "email" | "email address" | "e mail" | "mail" => Column::Email,
        "plus ones" | "plus one" | "plusones" | "plus 1" | "1" | "additional guests"
        | "extra guests" | "guests allowed" => Column::PlusOnes,
        "dietary" | "dietary requirements" | "dietary restrictions" | "diet" | "meal"
        | "meal preference" | "food" => Column::Dietary,
        "notes" | "note" | "comment" | "comments" | "remarks" => Column::Notes,
        "parts" | "part" | "events" | "sub events" | "attending" | "ceremonies" | "sessions" => {
            Column::Parts
        }
        _ => return None,
    })
}

/// Reduces a header to comparable words: lowercase, punctuation to spaces,
/// runs of whitespace collapsed. "Plus-Ones " and "plus_ones" both land on
/// "plus ones". A "+1" column reduces to "1", which `classify` accounts for.
fn normalize_header(raw: &str) -> String {
    let spaced: String = raw
        .trim_start_matches('\u{feff}')
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { ' ' })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Splits a multi-value cell. Organizers separate parts with either a
/// semicolon or a comma; inside a quoted CSV field both survive intact.
fn split_parts(cell: &str) -> Vec<String> {
    cell.split([';', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_plus_ones(cell: &str) -> Result<Option<i32>, String> {
    let cell = cell.trim();
    if cell.is_empty() {
        return Ok(None);
    }
    // Spreadsheets love turning an integer column into "2.0".
    let cell = cell.strip_suffix(".0").unwrap_or(cell);
    match cell.parse::<i32>() {
        Ok(n) if (0..=20).contains(&n) => Ok(Some(n)),
        Ok(n) if n < 0 => Err(format!("plus-ones can't be negative (got {n})")),
        Ok(n) => Err(format!("{n} plus-ones is more than the limit of 20")),
        Err(_) => Err(format!("{cell:?} isn't a whole number of plus-ones")),
    }
}

/// Parses a guest-list CSV. Row-level problems become `errors`; only a
/// structurally unusable file is an `Err`.
pub fn parse(input: &str) -> Result<ParsedCsv, CsvError> {
    // Excel's "CSV UTF-8" prepends a byte-order mark, which would otherwise
    // become part of the first header's name.
    let input = input.trim_start_matches('\u{feff}');
    if input.trim().is_empty() {
        return Err(CsvError::Empty);
    }

    // Excel writes CRLF, and the csv crate's line counter undercounts those
    // files by one — it calls the first data row of an Excel export line 1
    // rather than line 2. Every error below points the organizer at a row they
    // have to find by eye in a spreadsheet, so an off-by-one makes the whole
    // report worse than useless. Normalizing the terminators first keeps the
    // numbers honest. It also rewrites a newline typed inside a quoted note to
    // "\n", which is the form we want to store regardless.
    let input: Cow<str> = if input.contains("\r\n") {
        Cow::Owned(input.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(input)
    };

    let mut reader = csv::ReaderBuilder::new()
        // Guest lists are ragged: trailing commas, short last rows. Tolerate
        // it here and validate per field instead.
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(input.as_bytes());

    let headers = reader
        .headers()
        .map_err(|e| CsvError::Malformed(e.to_string()))?
        .clone();

    let mut mapping: Vec<Option<Column>> = Vec::with_capacity(headers.len());
    let mut ignored_columns = Vec::new();
    for header in headers.iter() {
        let column = classify(&normalize_header(header));
        if column.is_none() && !header.trim().is_empty() {
            ignored_columns.push(header.to_string());
        }
        // A duplicated meaning (two "Phone" columns) would otherwise have the
        // later one silently win; keep the first and ignore the rest.
        if let Some(c) = column
            && mapping.contains(&Some(c))
        {
            ignored_columns.push(header.to_string());
            mapping.push(None);
            continue;
        }
        mapping.push(column);
    }

    if !mapping.contains(&Some(Column::Name)) {
        return Err(CsvError::NoNameColumn);
    }

    let mut rows = Vec::new();
    let mut errors = Vec::new();

    for result in reader.records() {
        let record = match result {
            Ok(record) => record,
            Err(err) => return Err(CsvError::Malformed(err.to_string())),
        };
        // csv counts the header as line 1, which matches what Excel shows.
        let line = record.position().map_or(0, |p| p.line());

        // A row of empty cells is the blank line at the end of a spreadsheet,
        // not a guest the organizer forgot to name.
        if record.iter().all(|cell| cell.trim().is_empty()) {
            continue;
        }

        let cell = |want: Column| -> &str {
            mapping
                .iter()
                .position(|c| *c == Some(want))
                .and_then(|i| record.get(i))
                .unwrap_or("")
                .trim()
        };

        let name = cell(Column::Name);
        if name.is_empty() {
            errors.push(RowError { line, message: "no name".into() });
            continue;
        }
        if name.chars().count() > 200 {
            errors.push(RowError { line, message: "name is too long".into() });
            continue;
        }

        let raw_phone = cell(Column::Phone);
        let phone = match raw_phone {
            "" => None,
            raw => match phone::normalize(raw) {
                Some(number) => Some(number),
                None => {
                    errors.push(RowError {
                        line,
                        message: format!("{raw:?} isn't a phone number we can send to"),
                    });
                    continue;
                }
            },
        };

        let raw_email = cell(Column::Email);
        let email = match raw_email {
            "" => None,
            raw if is_emailish(raw) => Some(raw.to_lowercase()),
            raw => {
                errors.push(RowError { line, message: format!("{raw:?} isn't an email address") });
                continue;
            }
        };

        let plus_ones = match parse_plus_ones(cell(Column::PlusOnes)) {
            Ok(n) => n,
            Err(message) => {
                errors.push(RowError { line, message });
                continue;
            }
        };

        rows.push(ImportRow {
            line,
            name: name.to_string(),
            phone,
            email,
            plus_ones,
            dietary: truncate(cell(Column::Dietary), 500),
            notes: truncate(cell(Column::Notes), 1000),
            parts: split_parts(cell(Column::Parts)),
        });

        if rows.len() > MAX_ROWS {
            return Err(CsvError::TooManyRows(rows.len()));
        }
    }

    Ok(ParsedCsv { rows, errors, ignored_columns })
}

/// Deliberately not RFC 5322. The only question worth answering here is
/// whether this could be delivered; the mail server is the real authority.
/// Shared with the guests API so a typed-in address and an imported one are
/// held to the same standard.
pub fn is_emailish(value: &str) -> bool {
    let mut parts = value.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && value.len() <= 254
        && !value.contains(char::is_whitespace)
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_typical_wedding_spreadsheet() {
        let parsed = parse(
            "Name,Phone Number,Email,Plus Ones,Dietary,Notes\n\
             Adaeze Okafor,08066882563,ADA@example.com,2,Vegetarian,Bride's cousin\n\
             Tunde Bakare,+234 802 111 2222,,1,,\n",
        )
        .unwrap();

        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(
            parsed.rows[0],
            ImportRow {
                line: 2,
                name: "Adaeze Okafor".into(),
                phone: Some("+2348066882563".into()),
                email: Some("ada@example.com".into()),
                plus_ones: Some(2),
                dietary: "Vegetarian".into(),
                notes: "Bride's cousin".into(),
                parts: vec![],
            }
        );
        assert_eq!(parsed.rows[1].phone.as_deref(), Some("+2348021112222"));
        assert_eq!(parsed.rows[1].email, None);
    }

    #[test]
    fn recognises_headers_however_they_are_spelled() {
        let parsed = parse(
            "FULL NAME,mobile_number,E-Mail,plus-ones\n\
             Ngozi Eze,0803 111 2222,n@e.com,1\n",
        )
        .unwrap();
        let row = &parsed.rows[0];
        assert_eq!(row.name, "Ngozi Eze");
        assert_eq!(row.phone.as_deref(), Some("+2348031112222"));
        assert_eq!(row.email.as_deref(), Some("n@e.com"));
        assert_eq!(row.plus_ones, Some(1));
    }

    #[test]
    fn a_bad_row_does_not_sink_the_file() {
        let parsed = parse(
            "Name,Phone,Plus Ones\n\
             Good Guest,08066882563,1\n\
             Bad Phone,12345,0\n\
             ,08099999999,0\n\
             Too Many,08088888888,99\n\
             Also Good,08077777777,0\n",
        )
        .unwrap();

        assert_eq!(
            parsed.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["Good Guest", "Also Good"]
        );
        assert_eq!(parsed.errors.len(), 3);
        assert_eq!(parsed.errors[0].line, 3);
        assert!(parsed.errors[0].message.contains("phone number"), "{:?}", parsed.errors[0]);
        assert_eq!(parsed.errors[1], RowError { line: 4, message: "no name".into() });
        assert_eq!(parsed.errors[2].line, 5);
        assert!(parsed.errors[2].message.contains("limit of 20"), "{:?}", parsed.errors[2]);
    }

    #[test]
    fn handles_quoted_fields_and_excel_artefacts() {
        // A BOM, CRLF line endings, a quoted comma, an escaped quote, and the
        // "2.0" an integer column becomes after a round trip through Sheets.
        let parsed = parse(
            "\u{feff}Name,Notes,Plus Ones\r\n\
             \"Okafor, Adaeze\",\"Said \"\"maybe\"\", chase her\",2.0\r\n",
        )
        .unwrap();
        assert_eq!(parsed.rows.len(), 1, "{:?}", parsed.errors);
        assert_eq!(parsed.rows[0].name, "Okafor, Adaeze");
        assert_eq!(parsed.rows[0].notes, "Said \"maybe\", chase her");
        assert_eq!(parsed.rows[0].plus_ones, Some(2));
    }

    #[test]
    fn reads_parts_split_by_comma_or_semicolon() {
        let parsed = parse(
            "Name,Attending\n\
             A,\"Engagement, Reception\"\n\
             B,Engagement; Church Ceremony ;Reception\n\
             C,\n",
        )
        .unwrap();
        assert_eq!(parsed.rows[0].parts, ["Engagement", "Reception"]);
        assert_eq!(parsed.rows[1].parts, ["Engagement", "Church Ceremony", "Reception"]);
        assert!(parsed.rows[2].parts.is_empty());
    }

    /// An error is only actionable if it names the row the organizer can see,
    /// and real exports are CRLF. The csv crate's own line counter is off by
    /// one on those, so this pins the numbers against both line endings.
    #[test]
    fn error_lines_match_the_spreadsheet_whatever_the_line_endings() {
        let rows = "Name,Phone\nGood,08066882563\nBad,12345\nAlso Good,08033334444\nAlso Bad,999\n";
        for (label, input) in [("LF", rows.to_string()), ("CRLF", rows.replace('\n', "\r\n"))] {
            let parsed = parse(&input).unwrap();
            let lines: Vec<u64> = parsed.errors.iter().map(|e| e.line).collect();
            assert_eq!(lines, [3, 5], "{label}: bad rows are on lines 3 and 5");
            assert_eq!(parsed.rows.iter().map(|r| r.line).collect::<Vec<_>>(), [2, 4], "{label}");
        }
    }

    /// A newline inside a quoted cell shifts every following row down. The
    /// line numbers have to follow it, not the record count.
    #[test]
    fn a_newline_inside_a_cell_shifts_the_lines_after_it() {
        let parsed = parse(
            "Name,Notes\r\n\
             Adaeze,\"Sat with\r\nthe family\"\r\n\
             Bad Row,\r\n",
        )
        .unwrap();
        assert_eq!(parsed.rows[0].line, 2);
        assert_eq!(parsed.rows[0].notes, "Sat with\nthe family");
        assert_eq!(parsed.rows[1].line, 4, "the quoted newline pushed this row down");
    }

    #[test]
    fn an_absent_plus_ones_column_is_not_a_zero() {
        let parsed = parse("Name,Phone\nA,08066882563\n").unwrap();
        assert_eq!(parsed.rows[0].plus_ones, None);

        // Nor is an empty cell in a column that does exist. Only a written
        // "0" means zero.
        let parsed = parse("Name,Plus Ones\nA,\nB,0\n").unwrap();
        assert_eq!(parsed.rows[0].plus_ones, None);
        assert_eq!(parsed.rows[1].plus_ones, Some(0));
    }

    #[test]
    fn blank_rows_are_skipped_not_flagged() {
        let parsed = parse("Name,Phone\nReal Guest,08066882563\n,\n\n").unwrap();
        assert_eq!(parsed.rows.len(), 1);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    }

    #[test]
    fn unknown_columns_are_reported_not_silently_dropped() {
        let parsed = parse("Name,Table Number,Phone\nA,12,08066882563\n").unwrap();
        assert_eq!(parsed.ignored_columns, ["Table Number"]);
        assert_eq!(parsed.rows[0].phone.as_deref(), Some("+2348066882563"));
    }

    #[test]
    fn a_repeated_meaning_keeps_the_first_column() {
        let parsed = parse("Name,Phone,Mobile\nA,08066882563,08099999999\n").unwrap();
        assert_eq!(parsed.rows[0].phone.as_deref(), Some("+2348066882563"));
        assert_eq!(parsed.ignored_columns, ["Mobile"]);
    }

    #[test]
    fn ragged_rows_are_tolerated() {
        // Short row: the trailing columns are simply absent.
        let parsed = parse("Name,Phone,Notes\nA,08066882563\nB,,\n").unwrap();
        assert_eq!(parsed.rows.len(), 2, "{:?}", parsed.errors);
        assert_eq!(parsed.rows[0].notes, "");
        assert_eq!(parsed.rows[1].phone, None);
    }

    #[test]
    fn rejects_files_it_cannot_use() {
        assert_eq!(parse(""), Err(CsvError::Empty));
        assert_eq!(parse("   \n"), Err(CsvError::Empty));
        assert_eq!(parse("Table,Seat\n1,2\n"), Err(CsvError::NoNameColumn));
    }

    #[test]
    fn rejects_an_implausibly_large_list() {
        let mut csv = String::from("Name\n");
        for i in 0..=MAX_ROWS {
            csv.push_str(&format!("Guest {i}\n"));
        }
        assert_eq!(parse(&csv), Err(CsvError::TooManyRows(MAX_ROWS + 1)));
    }

    #[test]
    fn email_validation_is_about_deliverability_not_pedantry() {
        assert!(is_emailish("a@b.co"));
        assert!(is_emailish("first.last+tag@sub.example.com"));
        assert!(!is_emailish("no-at-sign.com"));
        assert!(!is_emailish("two@at@signs.com"));
        assert!(!is_emailish("@nolocal.com"));
        assert!(!is_emailish("no@domain"));
        assert!(!is_emailish("spaces in@example.com"));
    }
}
