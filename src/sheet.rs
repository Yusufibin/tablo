use std::borrow::Cow;
use std::cmp::Ordering;

use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellKind {
    Empty,
    Bool,
    Integer,
    Float,
    Date,
    Json,
    Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColKind {
    Empty,
    Bool,
    Integer,
    Float,
    Date,
    Json,
    Text,
    Mixed,
}

impl ColKind {
    pub fn label(self) -> &'static str {
        match self {
            ColKind::Empty => "vide",
            ColKind::Bool => "booléen",
            ColKind::Integer => "entier",
            ColKind::Float => "nombre",
            ColKind::Date => "date",
            ColKind::Json => "json",
            ColKind::Text => "texte",
            ColKind::Mixed => "mixte",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Sheet {
    /// All rows as in the file. When `has_header` is true, row 0 is the header.
    all: Vec<Vec<String>>,
    pub has_header: bool,
    pub headers: Vec<String>,
    pub kinds: Vec<ColKind>,
    pub widths: Vec<f32>,
}

impl Sheet {
    pub fn from_rows(mut all: Vec<Vec<String>>, has_header: bool) -> Self {
        normalize(&mut all);
        let mut sheet = Self {
            all,
            has_header,
            headers: Vec::new(),
            kinds: Vec::new(),
            widths: Vec::new(),
        };
        sheet.recompute();
        sheet
    }

    pub fn empty() -> Self {
        Self::from_rows(Vec::new(), true)
    }

    pub fn with_headers(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        let mut all = Vec::with_capacity(rows.len() + 1);
        all.push(headers);
        all.extend(rows);
        Self::from_rows(all, true)
    }

    pub fn col_count(&self) -> usize {
        self.headers.len()
    }

    pub fn row_count(&self) -> usize {
        let offset = self.offset();
        self.all.len().saturating_sub(offset)
    }

    pub fn set_has_header(&mut self, has_header: bool) {
        if self.has_header == has_header {
            return;
        }
        self.has_header = has_header;
        self.recompute();
    }

    pub fn header(&self, col: usize) -> &str {
        self.headers.get(col).map(String::as_str).unwrap_or("")
    }

    pub fn cell(&self, row: usize, col: usize) -> &str {
        self.row(row)
            .and_then(|r| r.get(col))
            .map(String::as_str)
            .unwrap_or("")
    }

    pub fn row(&self, row: usize) -> Option<&[String]> {
        self.all.get(self.offset() + row).map(Vec::as_slice)
    }

    pub fn filter(&self, query: &str) -> Vec<usize> {
        let n = self.row_count();
        if query.trim().is_empty() {
            return (0..n).collect();
        }
        let terms = parse_terms(query, &self.headers);
        (0..n)
            .filter(|&i| row_matches(self.row(i).unwrap_or(&[]), &terms))
            .collect()
    }

    pub fn sort_indices(&self, indices: &mut [usize], col: usize, ascending: bool) {
        if col >= self.col_count() {
            return;
        }
        let kind = self.kinds.get(col).copied().unwrap_or(ColKind::Text);
        indices.sort_by(|&a, &b| {
            let cmp = cmp_cells(self.cell(a, col), self.cell(b, col), kind);
            if ascending {
                cmp
            } else {
                cmp.reverse()
            }
        });
    }

    pub fn unique_count(&self, col: usize, rows: &[usize]) -> usize {
        let mut values: Vec<&str> = rows.iter().map(|&i| self.cell(i, col)).collect();
        values.sort_unstable();
        values.dedup();
        values.len()
    }

    pub fn to_csv(&self, rows: &[usize], delimiter: u8) -> Result<String, String> {
        let mut buf = Vec::new();
        {
            let mut w = csv::WriterBuilder::new()
                .delimiter(delimiter)
                .from_writer(&mut buf);
            w.write_record(&self.headers).map_err(|e| e.to_string())?;
            for &i in rows {
                if let Some(row) = self.row(i) {
                    w.write_record(row).map_err(|e| e.to_string())?;
                }
            }
            w.flush().map_err(|e| e.to_string())?;
        }
        String::from_utf8(buf).map_err(|e| e.to_string())
    }

    pub fn to_json(&self, rows: &[usize]) -> Result<String, String> {
        let mut out = Vec::with_capacity(rows.len());
        for &i in rows {
            let mut obj = serde_json::Map::new();
            if let Some(row) = self.row(i) {
                for (col, header) in self.headers.iter().enumerate() {
                    let raw = row.get(col).map(String::as_str).unwrap_or("");
                    obj.insert(header.clone(), json_cell(raw));
                }
            }
            out.push(Value::Object(obj));
        }
        serde_json::to_string_pretty(&Value::Array(out)).map_err(|e| e.to_string())
    }

    fn offset(&self) -> usize {
        if self.has_header && !self.all.is_empty() {
            1
        } else {
            0
        }
    }

    fn recompute(&mut self) {
        let width = self.all.iter().map(|r| r.len()).max().unwrap_or(0);
        if self.has_header {
            if let Some(first) = self.all.first() {
                self.headers = first.clone();
            } else {
                self.headers.clear();
            }
        } else {
            self.headers = (0..width).map(|i| format!("colonne {}", i + 1)).collect();
        }
        while self.headers.len() < width {
            self.headers
                .push(format!("colonne {}", self.headers.len() + 1));
        }
        self.kinds = infer_kinds(self);
        self.widths = estimate_widths(self);
    }
}

fn normalize(rows: &mut [Vec<String>]) {
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    for row in rows {
        row.resize(width, String::new());
    }
}

pub fn classify_cell(raw: &str) -> CellKind {
    let s = raw.trim();
    if s.is_empty() {
        return CellKind::Empty;
    }
    if is_bool(s) {
        return CellKind::Bool;
    }
    if is_int(s) {
        return CellKind::Integer;
    }
    if is_float(s) {
        return CellKind::Float;
    }
    if is_date(s) {
        return CellKind::Date;
    }
    if is_jsonish(s) {
        return CellKind::Json;
    }
    CellKind::Text
}

pub fn looks_like_header(row: &[String], rest: &[Vec<String>]) -> bool {
    if row.is_empty() {
        return false;
    }
    let textish = row
        .iter()
        .filter(|c| {
            matches!(
                classify_cell(c),
                CellKind::Text | CellKind::Empty | CellKind::Date | CellKind::Json
            )
        })
        .count();
    if textish * 2 < row.len() {
        return false;
    }
    if rest.is_empty() {
        return true;
    }
    let sample = rest.iter().take(24);
    let numeric_rest = sample
        .clone()
        .flat_map(|r| r.iter())
        .filter(|c| {
            matches!(
                classify_cell(c),
                CellKind::Integer | CellKind::Float | CellKind::Bool
            )
        })
        .count();
    let total_rest = sample
        .flat_map(|r| r.iter())
        .filter(|c| !c.trim().is_empty())
        .count();
    if total_rest > 0 && numeric_rest * 2 >= total_rest {
        return true;
    }
    let unique = {
        let mut v = row.to_vec();
        v.sort();
        v.dedup();
        v.len()
    };
    unique == row.len()
}

fn infer_kinds(sheet: &Sheet) -> Vec<ColKind> {
    let cols = sheet.col_count();
    let n = sheet.row_count();
    let sample = n.min(400);
    (0..cols)
        .map(|col| {
            let mut counts = [0u32; 7];
            let mut seen = 0u32;
            for row in 0..sample {
                let kind = classify_cell(sheet.cell(row, col));
                if kind == CellKind::Empty {
                    continue;
                }
                seen += 1;
                let idx = match kind {
                    CellKind::Bool => 0,
                    CellKind::Integer => 1,
                    CellKind::Float => 2,
                    CellKind::Date => 3,
                    CellKind::Json => 4,
                    CellKind::Text => 5,
                    CellKind::Empty => 6,
                };
                counts[idx] += 1;
            }
            if seen == 0 {
                return ColKind::Empty;
            }
            let mut best = 0;
            for i in 1..6 {
                if counts[i] > counts[best] {
                    best = i;
                }
            }
            if counts[best] * 4 < seen * 3 {
                return ColKind::Mixed;
            }
            match best {
                0 => ColKind::Bool,
                1 => {
                    if counts[2] > 0 {
                        ColKind::Float
                    } else {
                        ColKind::Integer
                    }
                }
                2 => ColKind::Float,
                3 => ColKind::Date,
                4 => ColKind::Json,
                _ => ColKind::Text,
            }
        })
        .collect()
}

fn estimate_widths(sheet: &Sheet) -> Vec<f32> {
    let cols = sheet.col_count();
    let sample = sheet.row_count().min(60);
    (0..cols)
        .map(|col| {
            let mut max_chars = sheet.header(col).chars().count().min(48);
            for row in 0..sample {
                let n = sheet.cell(row, col).chars().take(64).count();
                if n > max_chars {
                    max_chars = n;
                }
            }
            ((max_chars as f32) * 8.4 + 28.0).clamp(72.0, 420.0)
        })
        .collect()
}

struct Term {
    column: Option<usize>,
    needle: String,
    exclude: bool,
}

fn parse_terms(query: &str, headers: &[String]) -> Vec<Term> {
    let mut chars = query.chars().peekable();
    let mut terms = Vec::new();
    while let Some(ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        let mut exclude = chars.peek() == Some(&'-');
        if exclude {
            chars.next();
        }
        let mut text = String::new();
        let mut quoted = false;
        let mut colon = None;
        while let Some(&ch) = chars.peek() {
            if ch.is_whitespace() && !quoted {
                break;
            }
            chars.next();
            match ch {
                '"' => quoted = !quoted,
                '\\' if quoted && matches!(chars.peek(), Some('"' | '\\')) => {
                    text.push(chars.next().unwrap());
                }
                ':' if !quoted && colon.is_none() => {
                    colon = Some(text.len());
                    text.push(ch);
                }
                _ => text.push(ch),
            }
        }
        if exclude && text.is_empty() {
            text.push('-');
            exclude = false;
        }
        if text.is_empty() {
            continue;
        }
        let column = colon.and_then(|i| {
            if i + 1 == text.len() {
                return None;
            }
            let key = text[..i].to_lowercase();
            headers.iter().position(|h| h.to_lowercase() == key)
        });
        let needle = match (column, colon) {
            (Some(_), Some(i)) => text[i + 1..].to_lowercase(),
            _ => text.to_lowercase(),
        };
        terms.push(Term {
            column,
            needle,
            exclude,
        });
    }
    terms
}

fn row_matches(row: &[String], terms: &[Term]) -> bool {
    terms.iter().all(|term| {
        let matches = match term.column {
            Some(idx) => row
                .get(idx)
                .is_some_and(|cell| cell.to_lowercase().contains(&term.needle)),
            None => row
                .iter()
                .any(|cell| cell.to_lowercase().contains(&term.needle)),
        };
        matches != term.exclude
    })
}

/// Build a literal column query, including spaces, quotes and backslashes.
pub fn column_query(header: &str, value: &str) -> String {
    fn quote(text: &str) -> String {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }
    format!("{}:{}", quote(header), quote(value))
}

fn cmp_cells(a: &str, b: &str, kind: ColKind) -> Ordering {
    let a_empty = a.trim().is_empty();
    let b_empty = b.trim().is_empty();
    match (a_empty, b_empty) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    match kind {
        ColKind::Integer | ColKind::Float => match (parse_number(a), parse_number(b)) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => cmp_text(a, b),
        },
        ColKind::Bool => bool_rank(a).cmp(&bool_rank(b)),
        _ => cmp_text(a, b),
    }
}

fn cmp_text(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if let Ok(n) = t.parse::<f64>() {
        return Some(n);
    }
    if t.contains(',') && !t.contains('.') {
        return t.replace(',', ".").parse::<f64>().ok();
    }
    None
}

fn bool_rank(s: &str) -> u8 {
    if is_true(s) {
        1
    } else {
        0
    }
}

pub fn is_true(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "true" | "yes" | "oui" | "1"
    )
}

fn is_bool(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "true" | "false" | "yes" | "no" | "oui" | "non"
    )
}

fn is_int(s: &str) -> bool {
    let t = s.trim();
    let (sign, digits) = if let Some(rest) = t.strip_prefix('-') {
        (true, rest)
    } else {
        (false, t)
    };
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return false;
    }
    if !sign && digits.len() == 1 {
        return true;
    }
    digits.len() <= 18
}

fn is_float(s: &str) -> bool {
    let t = s.trim();
    if t.contains(',') && !t.contains('.') {
        if let Some((a, b)) = t.split_once(',') {
            let a = a.strip_prefix('-').unwrap_or(a);
            return !a.is_empty()
                && !b.is_empty()
                && a.bytes().all(|c| c.is_ascii_digit())
                && b.bytes().all(|c| c.is_ascii_digit());
        }
    }
    if !(t.contains('.') || t.contains('e') || t.contains('E')) {
        return false;
    }
    t.parse::<f64>().is_ok()
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() >= 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[0..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
    {
        return true;
    }
    b.len() == 10
        && b[2] == b'/'
        && b[5] == b'/'
        && b[0..2].iter().all(u8::is_ascii_digit)
        && b[3..5].iter().all(u8::is_ascii_digit)
        && b[6..10].iter().all(u8::is_ascii_digit)
}

fn is_jsonish(s: &str) -> bool {
    let t = s.trim();
    (t.starts_with('{') && t.ends_with('}')) || (t.starts_with('[') && t.ends_with(']'))
}

fn json_cell(s: &str) -> Value {
    let t = s.trim();
    if t.is_empty() {
        return Value::Null;
    }
    if is_bool(t) {
        return Value::Bool(is_true(t));
    }
    if is_int(t) {
        if let Ok(n) = t.parse::<i64>() {
            return n.into();
        }
    }
    if let Some(n) = parse_number(t) {
        if let Some(num) = serde_json::Number::from_f64(n) {
            return Value::Number(num);
        }
    }
    if is_jsonish(t) {
        if let Ok(v) = serde_json::from_str(t) {
            return v;
        }
    }
    Value::String(s.to_string())
}

pub fn truncate(s: &str, max_chars: usize) -> Cow<'_, str> {
    match s.char_indices().nth(max_chars) {
        None => Cow::Borrowed(s),
        Some((i, _)) => {
            let mut out = s[..i].to_string();
            out.push('…');
            Cow::Owned(out)
        }
    }
}

pub fn fmt_int(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(ch);
    }
    out
}

pub fn fmt_bytes(n: u64) -> String {
    const KO: f64 = 1024.0;
    const MO: f64 = KO * 1024.0;
    if n < 1024 {
        format!("{n} o")
    } else if (n as f64) < MO {
        format!("{:.1} Ko", n as f64 / KO)
    } else {
        format!("{:.1} Mo", n as f64 / MO)
    }
}

pub fn pretty_json_if_possible(s: &str) -> Option<String> {
    let t = s.trim();
    if !is_jsonish(t) {
        return None;
    }
    let value: Value = serde_json::from_str(t).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

fn as_json_object_row(sheet: &Sheet, row: usize) -> Value {
    let mut obj = serde_json::Map::new();
    if let Some(cells) = sheet.row(row) {
        for (col, header) in sheet.headers.iter().enumerate() {
            let raw = cells.get(col).map(String::as_str).unwrap_or("");
            obj.insert(header.clone(), json_cell(raw));
        }
    }
    Value::Object(obj)
}

pub fn row_as_json_string(sheet: &Sheet, row: usize) -> String {
    serde_json::to_string_pretty(&as_json_object_row(sheet, row))
        .unwrap_or_else(|_| json!({}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_and_column_query() {
        let sheet = Sheet::with_headers(
            vec!["nom".into(), "ville".into()],
            vec![
                vec!["Ada".into(), "Paris".into()],
                vec!["Linus".into(), "Helsinki".into()],
                vec!["Grace".into(), "Paris".into()],
            ],
        );
        assert_eq!(sheet.filter("paris"), vec![0, 2]);
        assert_eq!(sheet.filter("ville:helsinki"), vec![1]);
        assert_eq!(sheet.filter("nom:ada ville:paris"), vec![0]);
    }

    #[test]
    fn filter_phrases_exclusions_and_unicode_headers() {
        let sheet = Sheet::with_headers(
            vec!["Nom complet".into(), "ville".into(), "État".into()],
            vec![
                vec!["Ada Lovelace".into(), "New York".into(), "Actif".into()],
                vec!["Grace Hopper".into(), "York New".into(), "Inactif".into()],
                vec!["Ada Byron".into(), "New York".into(), "Inactif".into()],
            ],
        );
        assert_eq!(sheet.filter("\"New York\""), vec![0, 2]);
        assert_eq!(sheet.filter("ville:\"new york\" -ÉTAT:inactif"), vec![0]);
        assert_eq!(sheet.filter("\"Nom complet\":\"Grace Hopper\""), vec![1]);
        assert_eq!(sheet.filter("-\"New York\""), vec![1]);
        assert_eq!(sheet.filter("ada -byron"), vec![0]);
        // Keep the search usable while the closing quote is still being typed.
        assert_eq!(sheet.filter("ville:\"New York"), vec![0, 2]);
        assert_eq!(sheet.filter("   "), vec![0, 1, 2]);
    }

    #[test]
    fn context_query_preserves_literal_values() {
        let cases = [
            ("Nom complet", "Ada Lovelace"),
            ("note:source", "dit \"bonjour\" à Paris"),
            ("chemin", r"C:\Users\Ada Lovelace"),
            ("lien", "https://example.test/a:b"),
            ("code", "-inactif"),
        ];
        for (header, value) in cases {
            let sheet = Sheet::with_headers(
                vec![header.into()],
                vec![vec![value.into()], vec!["autre valeur".into()]],
            );
            assert_eq!(
                sheet.filter(&column_query(header, value)),
                vec![0],
                "{value}"
            );
        }
    }

    #[test]
    fn unknown_column_and_quoted_colon_remain_literal() {
        let sheet = Sheet::with_headers(
            vec!["note".into()],
            vec![vec!["ville:Paris".into()], vec!["autre".into()]],
        );
        assert_eq!(sheet.filter("ville:paris"), vec![0]);
        assert_eq!(sheet.filter("\"ville:paris\""), vec![0]);
        assert_eq!(sheet.filter("-ville:paris"), vec![1]);
    }

    #[test]
    fn numeric_sort() {
        let sheet = Sheet::with_headers(
            vec!["n".into()],
            vec![
                vec!["10".into()],
                vec!["2".into()],
                vec!["".into()],
                vec!["3".into()],
            ],
        );
        let mut idx = vec![0, 1, 2, 3];
        sheet.sort_indices(&mut idx, 0, true);
        assert_eq!(idx, vec![1, 3, 0, 2]);
    }

    #[test]
    fn classify_french_float() {
        assert_eq!(classify_cell("12,50"), CellKind::Float);
        assert_eq!(classify_cell("2024-01-02"), CellKind::Date);
        assert_eq!(classify_cell("true"), CellKind::Bool);
        assert_eq!(classify_cell("{\"a\":1}"), CellKind::Json);
    }

    #[test]
    fn header_toggle_keeps_rows() {
        let mut sheet = Sheet::from_rows(
            vec![vec!["a".into(), "b".into()], vec!["1".into(), "2".into()]],
            true,
        );
        assert_eq!(sheet.row_count(), 1);
        sheet.set_has_header(false);
        assert_eq!(sheet.row_count(), 2);
        assert_eq!(sheet.header(0), "colonne 1");
    }
}
