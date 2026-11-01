use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::sheet::{looks_like_header, Sheet};

#[derive(Clone, Debug)]
pub enum DocKind {
    Csv { delimiter: char },
    Json { tabular: bool },
    Ndjson,
}

impl DocKind {
    pub fn label(&self) -> &'static str {
        match self {
            DocKind::Csv { .. } => "CSV",
            DocKind::Json { .. } => "JSON",
            DocKind::Ndjson => "NDJSON",
        }
    }

    pub fn delimiter(&self) -> u8 {
        match self {
            DocKind::Csv { delimiter } => *delimiter as u8,
            _ => b',',
        }
    }
}

#[derive(Clone, Debug)]
pub struct Opened {
    pub path: PathBuf,
    pub name: String,
    pub bytes: u64,
    pub encoding: &'static str,
    pub kind: DocKind,
    pub sheet: Sheet,
    pub json: Option<Value>,
    pub notes: Vec<String>,
}

pub fn open_path(path: &Path) -> Result<Opened, String> {
    let bytes = fs::read(path).map_err(|e| format!("Lecture impossible : {e}"))?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("fichier")
        .to_string();
    open_bytes(path.to_path_buf(), name, &bytes)
}

pub fn open_bytes(path: PathBuf, name: String, bytes: &[u8]) -> Result<Opened, String> {
    let size = bytes.len() as u64;
    let (text, encoding) = decode(bytes);
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let hint = match ext.as_str() {
        "json" => Hint::Json,
        "ndjson" | "jsonl" => Hint::Ndjson,
        "tsv" => Hint::Csv(Some(b'\t')),
        "csv" | "txt" => Hint::Csv(None),
        _ => Hint::Unknown,
    };
    parse_text(path, name, size, encoding, &text, hint)
}

#[derive(Clone, Copy)]
enum Hint {
    Unknown,
    Csv(Option<u8>),
    Json,
    Ndjson,
}

fn parse_text(
    path: PathBuf,
    name: String,
    bytes: u64,
    encoding: &'static str,
    text: &str,
    hint: Hint,
) -> Result<Opened, String> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    match hint {
        Hint::Json => open_json(path, name, bytes, encoding, text),
        Hint::Ndjson => open_ndjson(path, name, bytes, encoding, text),
        Hint::Csv(forced) => open_csv(path, name, bytes, encoding, text, forced),
        Hint::Unknown => {
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                match open_json(path.clone(), name.clone(), bytes, encoding, text) {
                    Ok(doc) => Ok(doc),
                    Err(_) if looks_ndjson(text) => open_ndjson(path, name, bytes, encoding, text),
                    Err(e) => Err(e),
                }
            } else if looks_ndjson(text) {
                open_ndjson(path, name, bytes, encoding, text)
            } else {
                open_csv(path, name, bytes, encoding, text, None)
            }
        }
    }
}

fn open_csv(
    path: PathBuf,
    name: String,
    bytes: u64,
    encoding: &'static str,
    text: &str,
    forced: Option<u8>,
) -> Result<Opened, String> {
    let delimiter = forced.unwrap_or_else(|| sniff_delimiter(text));
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .has_headers(false)
        .from_reader(text.as_bytes());
    let mut all: Vec<Vec<String>> = Vec::new();
    for rec in reader.records() {
        let rec = rec.map_err(|e| format!("CSV invalide : {e}"))?;
        all.push(rec.iter().map(|s| s.to_string()).collect());
    }
    let mut notes = Vec::new();
    if all.is_empty() {
        notes.push("Fichier vide.".into());
    }
    let has_header = if all.len() >= 2 {
        looks_like_header(&all[0], &all[1..])
    } else {
        true
    };
    if !has_header {
        notes.push("Première ligne lue comme donnée, pas comme en-tête.".into());
    }
    let delim_ch = delimiter as char;
    if delim_ch == ';' {
        notes.push("Séparateur point-virgule détecté.".into());
    } else if delim_ch == '\t' {
        notes.push("Séparateur tabulation détecté.".into());
    }
    Ok(Opened {
        path,
        name,
        bytes,
        encoding,
        kind: DocKind::Csv {
            delimiter: delim_ch,
        },
        sheet: Sheet::from_rows(all, has_header),
        json: None,
        notes,
    })
}

fn open_json(
    path: PathBuf,
    name: String,
    bytes: u64,
    encoding: &'static str,
    text: &str,
) -> Result<Opened, String> {
    let value = match serde_json::from_str(text.trim_start_matches('\u{feff}').trim()) {
        Ok(v) => v,
        Err(_) if looks_ndjson(text) => {
            return open_ndjson(path, name, bytes, encoding, text);
        }
        Err(e) => return Err(format!("JSON invalide : {e}")),
    };
    let (sheet, tabular, notes) = json_to_sheet(&value);
    Ok(Opened {
        path,
        name,
        bytes,
        encoding,
        kind: DocKind::Json { tabular },
        sheet,
        json: Some(value),
        notes,
    })
}

fn open_ndjson(
    path: PathBuf,
    name: String,
    bytes: u64,
    encoding: &'static str,
    text: &str,
) -> Result<Opened, String> {
    let mut values = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value =
            serde_json::from_str(line).map_err(|e| format!("NDJSON ligne {} : {e}", i + 1))?;
        values.push(v);
    }
    if values.is_empty() {
        return Err("NDJSON vide.".into());
    }
    let array = Value::Array(values);
    let (sheet, _tabular, mut notes) = json_to_sheet(&array);
    notes.insert(0, "Fichier NDJSON (une valeur JSON par ligne).".into());
    Ok(Opened {
        path,
        name,
        bytes,
        encoding,
        kind: DocKind::Ndjson,
        sheet,
        json: Some(array),
        notes,
    })
}

fn json_to_sheet(value: &Value) -> (Sheet, bool, Vec<String>) {
    let mut notes = Vec::new();
    if let Some(sheet) = try_records(value, &mut notes) {
        return (sheet, true, notes);
    }
    notes.push("JSON imbriqué affiché en feuilles (chemin / type / valeur).".into());
    (leaves_table(value), false, notes)
}

fn try_records(value: &Value, notes: &mut Vec<String>) -> Option<Sheet> {
    match value {
        Value::Array(arr) => records_from_array(arr, notes),
        Value::Object(map) => {
            for key in ["data", "items", "records", "rows", "results", "values"] {
                if let Some(Value::Array(arr)) = map.get(key) {
                    if arr.iter().any(|v| v.is_object() || v.is_array()) {
                        notes.push(format!("Tableau lu depuis « {key} »."));
                        return records_from_array(arr, notes);
                    }
                }
            }
            let object_values: Vec<(&String, &Value)> =
                map.iter().filter(|(_, v)| v.is_object()).collect();
            let similar = objects_look_like_records(
                &object_values.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
            );
            if object_values.len() >= 3 && object_values.len() * 4 >= map.len() * 3 && similar {
                notes.push("Objet converti en lignes (colonne _key).".into());
                let mut arr = Vec::new();
                for (k, v) in map {
                    if let Value::Object(inner) = v {
                        let mut obj = inner.clone();
                        obj.insert("_key".into(), Value::String(k.clone()));
                        arr.push(Value::Object(obj));
                    }
                }
                return records_from_array(&arr, notes);
            }
            None
        }
        _ => {
            let sheet =
                Sheet::with_headers(vec!["valeur".into()], vec![vec![display_scalar(value)]]);
            Some(sheet)
        }
    }
}

fn objects_look_like_records(values: &[&Value]) -> bool {
    let keys: Vec<Vec<&str>> = values
        .iter()
        .take(12)
        .filter_map(|v| {
            v.as_object()
                .map(|o| o.keys().map(String::as_str).collect())
        })
        .collect();
    if keys.len() < 2 {
        return false;
    }
    let first = &keys[0];
    if first.is_empty() {
        return false;
    }
    let overlap = keys
        .iter()
        .skip(1)
        .filter(|other| {
            other.iter().filter(|k| first.contains(k)).count() * 2 >= first.len().min(other.len())
        })
        .count();
    overlap + 1 >= keys.len().saturating_sub(1)
}

fn records_from_array(arr: &[Value], notes: &mut Vec<String>) -> Option<Sheet> {
    if arr.is_empty() {
        return Some(Sheet::empty());
    }
    if arr.iter().all(is_scalar) {
        let rows = arr.iter().map(|v| vec![display_scalar(v)]).collect();
        return Some(Sheet::with_headers(vec!["valeur".into()], rows));
    }
    if arr.iter().all(|v| v.is_array()) {
        let mut rows: Vec<Vec<String>> = arr
            .iter()
            .map(|v| v.as_array().unwrap().iter().map(display_scalar).collect())
            .collect();
        let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        for row in &mut rows {
            row.resize(width, String::new());
        }
        let has_header = rows
            .first()
            .map(|r| looks_like_header(r, rows.get(1..).unwrap_or(&[])))
            .unwrap_or(false);
        return Some(Sheet::from_rows(rows, has_header));
    }
    if arr.iter().all(|v| v.is_object()) {
        let mut headers: Vec<String> = Vec::new();
        let mut rows: Vec<Vec<String>> = Vec::with_capacity(arr.len());
        for value in arr {
            let mut pairs = Vec::new();
            flatten_value(value, "", 0, &mut pairs);
            for (key, _) in &pairs {
                if !headers.iter().any(|h| h == key) {
                    headers.push(key.clone());
                }
            }
            let mut row = vec![String::new(); headers.len()];
            for (key, val) in pairs {
                if let Some(idx) = headers.iter().position(|h| h == &key) {
                    if idx < row.len() {
                        row[idx] = val;
                    }
                }
            }
            rows.push(row);
        }
        for row in &mut rows {
            row.resize(headers.len(), String::new());
        }
        if headers.iter().any(|h| h.contains('.')) {
            notes.push("Champs imbriqués aplatis (notation pointée).".into());
        }
        return Some(Sheet::with_headers(headers, rows));
    }
    None
}

fn leaves_table(value: &Value) -> Sheet {
    let mut rows = Vec::new();
    walk_leaves(String::new(), value, &mut rows);
    if rows.is_empty() {
        rows.push(vec![
            String::new(),
            type_name(value).into(),
            display_leaf(value),
        ]);
    }
    Sheet::with_headers(vec!["chemin".into(), "type".into(), "valeur".into()], rows)
}

fn walk_leaves(path: String, value: &Value, rows: &mut Vec<Vec<String>>) {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                rows.push(vec![path, "object".into(), "{}".into()]);
                return;
            }
            for (k, v) in map {
                let next = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                walk_leaves(next, v, rows);
            }
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                rows.push(vec![path, "array".into(), "[]".into()]);
                return;
            }
            if arr.iter().all(is_scalar) {
                rows.push(vec![path, "array".into(), display_array_scalars(arr)]);
                return;
            }
            for (i, v) in arr.iter().enumerate() {
                let next = if path.is_empty() {
                    format!("[{i}]")
                } else {
                    format!("{path}[{i}]")
                };
                walk_leaves(next, v, rows);
            }
        }
        _ => rows.push(vec![path, type_name(value).into(), display_leaf(value)]),
    }
}

fn flatten_value(value: &Value, prefix: &str, depth: usize, out: &mut Vec<(String, String)>) {
    const MAX_DEPTH: usize = 8;
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                out.push((prefix.to_string(), "{}".into()));
                return;
            }
            if depth >= MAX_DEPTH {
                out.push((prefix.to_string(), compact(value)));
                return;
            }
            for (k, v) in map {
                let next = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_value(v, &next, depth + 1, out);
            }
        }
        Value::Array(_) => out.push((prefix.to_string(), compact(value))),
        _ => out.push((prefix.to_string(), display_scalar(value))),
    }
}

fn is_scalar(value: &Value) -> bool {
    !matches!(value, Value::Object(_) | Value::Array(_))
}

fn display_scalar(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => compact(other),
    }
}

fn display_leaf(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        _ => display_scalar(value),
    }
}

fn display_array_scalars(arr: &[Value]) -> String {
    compact(&Value::Array(arr.to_vec()))
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn decode(bytes: &[u8]) -> (String, &'static str) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (
            String::from_utf8_lossy(&bytes[3..]).into_owned(),
            "UTF-8 BOM",
        );
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_string(), "UTF-8"),
        Err(_) => (bytes.iter().map(|&b| b as char).collect(), "Latin-1"),
    }
}

fn looks_ndjson(text: &str) -> bool {
    let mut seen = 0;
    let mut ok = 0;
    for line in text.lines().take(40) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        seen += 1;
        if (line.starts_with('{') || line.starts_with('['))
            && serde_json::from_str::<Value>(line).is_ok()
        {
            ok += 1;
        }
    }
    seen >= 2 && ok * 2 >= seen
}

pub fn sniff_delimiter(text: &str) -> u8 {
    let candidates = *b",;\t|";
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .take(16)
        .collect();
    if lines.is_empty() {
        return b',';
    }
    let mut best = b',';
    let mut best_score = 0i32;
    for delim in candidates {
        let counts: Vec<usize> = lines.iter().map(|l| field_count(l, delim)).collect();
        if counts.iter().all(|&n| n <= 1) {
            continue;
        }
        let mut freq = std::collections::HashMap::new();
        for n in &counts {
            *freq.entry(*n).or_insert(0) += 1;
        }
        let (mode, hit) = freq.into_iter().max_by_key(|(_, n)| *n).unwrap_or((1, 0));
        if mode < 2 {
            continue;
        }
        let score = hit * 10 + mode as i32;
        if score > best_score {
            best_score = score;
            best = delim;
        }
    }
    best
}

fn field_count(line: &str, delim: u8) -> usize {
    let mut n = 1;
    let mut in_quotes = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' {
            if in_quotes && i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                i += 2;
                continue;
            }
            in_quotes = !in_quotes;
        } else if c == delim && !in_quotes {
            n += 1;
        }
        i += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn load(name: &str, text: &str) -> Opened {
        open_bytes(PathBuf::from(name), name.into(), text.as_bytes()).unwrap()
    }

    #[test]
    fn sniffs_semicolon() {
        let sample = "nom;ville;montant\nAda;Paris;12,5\nLinus;Helsinki;3\n";
        assert_eq!(sniff_delimiter(sample), b';');
        let doc = load("people.csv", sample);
        assert_eq!(doc.sheet.col_count(), 3);
        assert_eq!(doc.sheet.row_count(), 2);
        assert_eq!(doc.sheet.cell(0, 0), "Ada");
    }

    #[test]
    fn json_array_flatten() {
        let text = r#"[{"user":{"name":"Ada","city":"Paris"},"ok":true}]"#;
        let doc = load("people.json", text);
        assert!(doc.sheet.headers.iter().any(|h| h == "user.name"));
        assert_eq!(
            doc.sheet
                .cell(0, doc.sheet.headers.iter().position(|h| h == "ok").unwrap()),
            "true"
        );
    }

    #[test]
    fn ndjson_union_keys() {
        let text = "{\"x\":1}\n{\"x\":2,\"y\":3}\n";
        let doc = load("rows.ndjson", text);
        assert_eq!(doc.sheet.row_count(), 2);
        assert!(doc.sheet.headers.iter().any(|h| h == "y"));
        assert_eq!(
            doc.sheet
                .cell(0, doc.sheet.headers.iter().position(|h| h == "y").unwrap()),
            ""
        );
        assert_eq!(
            doc.sheet
                .cell(1, doc.sheet.headers.iter().position(|h| h == "y").unwrap()),
            "3"
        );
    }

    #[test]
    fn wrapped_data_array() {
        let text = r#"{"data":[{"id":1},{"id":2}]}"#;
        let doc = load("wrap.json", text);
        assert_eq!(doc.sheet.row_count(), 2);
        assert_eq!(doc.sheet.cell(1, 0), "2");
    }

    #[test]
    fn latin1_fallback() {
        let bytes: Vec<u8> = b"nom\n".iter().copied().chain([0xC9, b'\n']).collect(); // É
        let doc = open_bytes(PathBuf::from("a.csv"), "a.csv".into(), &bytes).unwrap();
        assert_eq!(doc.encoding, "Latin-1");
        assert_eq!(doc.sheet.cell(0, 0), "É");
    }

    #[test]
    fn samples_open() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples");
        let people = open_path(&root.join("people.csv")).unwrap();
        assert_eq!(people.sheet.row_count(), 5);
        assert_eq!(people.kind.delimiter(), b';');
        let products = open_path(&root.join("products.json")).unwrap();
        assert_eq!(products.sheet.row_count(), 2);
        assert!(products.sheet.headers.iter().any(|h| h == "meta.stock"));
        let config = open_path(&root.join("config.json")).unwrap();
        assert!(config.json.is_some());
        match config.kind {
            DocKind::Json { tabular } => assert!(
                !tabular,
                "config.json est un objet mixte, pas un tableau de records"
            ),
            other => panic!("attendu JSON, obtenu {other:?}"),
        }
        assert!(config.sheet.headers.iter().any(|h| h == "chemin"));
        let events = open_path(&root.join("events.ndjson")).unwrap();
        assert_eq!(events.sheet.row_count(), 3);
    }

    #[test]
    fn sniffs_tab() {
        let sample = "a\tb\tc\n1\t2\t3\n4\t5\t6\n";
        assert_eq!(sniff_delimiter(sample), b'\t');
        let doc = load("grid.tsv", sample);
        assert_eq!(doc.kind.delimiter(), b'\t');
        assert_eq!(doc.sheet.col_count(), 3);
        assert_eq!(doc.sheet.cell(0, 1), "2");
    }

    #[test]
    fn workspace_csv_and_nested_json() {
        let csv = PathBuf::from("/home/kazuto/Bureau/h/cinetpay-payout-env.csv");
        if csv.exists() {
            let doc = open_path(&csv).unwrap();
            assert!(doc.sheet.row_count() > 20);
            assert_eq!(doc.sheet.header(0), "repository");
        }
        let json = PathBuf::from("/home/kazuto/Bureau/h/waves6.json");
        if json.exists() {
            let doc = open_path(&json).unwrap();
            assert!(doc.json.is_some());
            assert!(doc.sheet.row_count() > 0);
        }
    }
}
