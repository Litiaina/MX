//! Bounded, non-executing spreadsheet readers. Never allocate an Excel Range
//! from a declared worksheet dimension (often enormous formatting-only areas).
use calamine::{DataRef, Reader, Xlsx};
use quick_xml::{Reader as XmlReader, events::Event};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};

pub const MAX_UPLOAD: usize = 64 * 1024 * 1024;
const MAX_ROWS: usize = 100_000;
const MAX_TEXT: usize = 64 * 1024 * 1024;
const MAX_COLUMNS: u32 = 512;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Cell {
    pub text: String,
    pub kind: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceRow {
    pub row: u32,
    pub cells: BTreeMap<u32, Cell>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Sheet {
    pub name: String,
    pub rows: Vec<SourceRow>,
    pub error: Option<String>,
}

fn push(
    sheet: &mut Sheet,
    mut row: SourceRow,
    count: &mut usize,
    text: &mut usize,
) -> Result<(), String> {
    row.cells.retain(|_, c| !c.text.trim().is_empty());
    if row.cells.is_empty() {
        return Ok(());
    }
    *count += 1;
    *text += row.cells.values().map(|c| c.text.len() + 32).sum::<usize>();
    if *count > MAX_ROWS || *text > MAX_TEXT || row.cells.keys().any(|c| *c > MAX_COLUMNS) {
        return Err("This workbook exceeds the import safety budget (100,000 populated rows, 512 populated columns or 64 MiB of cell text). Split it into smaller imports; formatting-only rows do not count.".into());
    }
    sheet.rows.push(row);
    Ok(())
}

fn archive(bytes: &[u8]) -> Result<zip::ZipArchive<Cursor<&[u8]>>, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| "This is not a valid spreadsheet archive.")?;
    if zip.len() > 4096 {
        return Err("Spreadsheet has too many archive entries.".into());
    }
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let declared = entry.size();
        total = total.saturating_add(entry.size());
        if total > 256 * 1024 * 1024 {
            return Err(
                "Spreadsheet expands beyond the 256 MiB import safety budget. Split the workbook."
                    .into(),
            );
        }
        // Do not trust central-directory sizes: verify actual decompression
        // with a hard bound before handing this immutable archive to a reader.
        let actual = std::io::copy(&mut entry.by_ref().take(declared + 1), &mut std::io::sink())
            .map_err(|e| format!("Corrupt spreadsheet archive: {e}"))?;
        if actual != declared {
            return Err("Spreadsheet archive has inconsistent expanded-size metadata.".into());
        }
    }
    // Calamine reserves shared strings from this untrusted attribute.
    if let Ok(mut entry) = zip.by_name("xl/sharedStrings.xml") {
        let mut prefix = Vec::new();
        entry
            .by_ref()
            .take(16 * 1024)
            .read_to_end(&mut prefix)
            .map_err(|e| e.to_string())?;
        let mut xml = XmlReader::from_reader(prefix.as_slice());
        let mut validated = false;
        loop {
            match xml.read_event() {
                Ok(Event::Start(e) | Event::Empty(e)) => {
                    for a in e.attributes().flatten() {
                        if a.key.as_ref() == b"uniqueCount"
                            && std::str::from_utf8(&a.value)
                                .ok()
                                .and_then(|n| n.parse::<u64>().ok())
                                .is_none_or(|n| n > 1_000_000)
                        {
                            return Err(
                                "Invalid or excessive spreadsheet shared-string count.".into()
                            );
                        }
                    }
                    validated = true;
                    break;
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
        if !validated {
            return Err("Invalid or oversized shared-string header.".into());
        }
    }
    Ok(zip)
}

pub fn parse(name: &str, bytes: &[u8], delimiter: Option<u8>) -> Result<Vec<Sheet>, String> {
    if bytes.len() > MAX_UPLOAD {
        return Err("Spreadsheet imports accept files up to 64 MiB compressed; split larger workbooks. This is an import safety budget, not a Drive upload limit.".into());
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let (mut count, mut text) = (0, 0);
    match ext.as_str() {
        "xlsx" => {
            archive(bytes)?;
            let mut book =
                Xlsx::new(Cursor::new(bytes)).map_err(|e| format!("Cannot read XLSX: {e}"))?;
            let names = book.sheet_names().to_vec();
            if names.len() > 128 {
                return Err("Select a workbook with at most 128 sheets.".into());
            }
            let mut sheets = Vec::new();
            for name in names {
                let (before_count, before_text) = (count, text);
                let outcome: Result<Sheet, String> = (|| {
                    let mut sheet = Sheet {
                        name: name.clone(),
                        rows: Vec::new(),
                        error: None,
                    };
                    let mut reader = book
                        .worksheet_cells_reader(&name)
                        .map_err(|e| e.to_string())?;
                    let mut row = SourceRow {
                        row: 0,
                        cells: BTreeMap::new(),
                    };
                    while let Some(record) = reader
                        .next_cell_with_formula_metadata()
                        .map_err(|e| format!("{name}: {e}"))?
                    {
                        let (r, c) = record.pos;
                        let r = r.checked_add(1).ok_or("Invalid worksheet row.")?;
                        if row.row != r {
                            if row.row > r {
                                return Err("Worksheet rows are out of order.".into());
                            }
                            push(&mut sheet, row, &mut count, &mut text)?;
                            row = SourceRow {
                                row: r,
                                cells: BTreeMap::new(),
                            };
                        }
                        let value = &record.value;
                        let cell = match value {
                        DataRef::Empty if record.formula.is_some() => Cell { text: "Formula has no cached value. Recalculate and save it in a spreadsheet application first.".into(), kind: "error".into() },
                        DataRef::Empty => continue,
                        DataRef::DateTime(v) if v.is_duration() => match v.as_duration(){Some(duration)=>Cell{text:duration.to_string(),kind:"text".into()},None=>Cell{text:"Invalid spreadsheet duration.".into(),kind:"error".into()}},
                        DataRef::DateTime(v) => match v.as_datetime() {Some(date)=>Cell{text:if date.time()==chrono::NaiveTime::MIN{date.date().to_string()}else{date.to_string()},kind:"date".into()},None=>Cell{text:format!("Invalid spreadsheet date ({v})."),kind:"error".into()}},
                        DataRef::DateTimeIso(v) => Cell { text: v.clone(), kind: "date".into() },
                        DataRef::Error(v) => Cell { text: v.to_string(), kind: "error".into() },
                        DataRef::Int(v) => Cell { text: v.to_string(), kind: "text".into() },
                        DataRef::Float(v) => Cell { text: v.to_string(), kind: "text".into() },
                        DataRef::Bool(v) => Cell { text: v.to_string(), kind: "text".into() },
                        DataRef::String(v) | DataRef::DurationIso(v) => Cell { text: v.clone(), kind: "text".into() },
                        DataRef::SharedString(v) => Cell { text: (*v).into(), kind: "text".into() },
                    };
                        if !cell.text.trim().is_empty() {
                            row.cells
                                .insert(c.checked_add(1).ok_or("Invalid worksheet column.")?, cell);
                        }
                        if row.cells.len() > MAX_COLUMNS as usize
                            || row.cells.values().map(|c| c.text.len()).sum::<usize>() > 1024 * 1024
                        {
                            return Err(
                                "A spreadsheet row exceeds the import safety budget.".into()
                            );
                        }
                    }
                    push(&mut sheet, row, &mut count, &mut text)?;
                    Ok(sheet)
                })();
                match outcome {
                    Ok(sheet) => sheets.push(sheet),
                    Err(error) => {
                        count = before_count;
                        text = before_text;
                        sheets.push(Sheet {
                            name,
                            rows: Vec::new(),
                            error: Some(error),
                        });
                    }
                }
            }
            Ok(sheets)
        }
        "csv" | "tsv" => {
            let mut sheet = Sheet {
                name: "Data".into(),
                rows: Vec::new(),
                error: None,
            };
            let delimiter = delimiter.unwrap_or(if ext == "tsv" { b'\t' } else { b',' });
            let mut reader = csv::ReaderBuilder::new()
                .delimiter(delimiter)
                .has_headers(false)
                .flexible(true)
                .from_reader(bytes);
            let (mut offset, mut physical_line) = (0usize, 1u32);
            for record in reader.byte_records() {
                let record = record.map_err(|e| format!("Malformed delimited file: {e}"))?;
                let mut start = record
                    .position()
                    .map(|p| p.byte() as usize)
                    .unwrap_or(offset);
                // csv-core may stop on CR and leave the LF for the next
                // record. Its byte position can therefore precede that
                // record's first cell. Skip only inter-record terminators.
                while matches!(bytes.get(start), Some(b'\r' | b'\n')) {
                    start += 1;
                }
                while offset < start {
                    if bytes[offset] == b'\n'
                        || (bytes[offset] == b'\r' && bytes.get(offset + 1) != Some(&b'\n'))
                    {
                        physical_line += 1;
                    }
                    offset += 1;
                }
                let row = physical_line;
                let cells = record
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| !v.is_empty())
                    .map(|(i, v)| {
                        let cell = match std::str::from_utf8(v) {
                            Ok(v) => Cell {
                                text: v.trim_start_matches('\u{feff}').into(),
                                kind: "text".into(),
                            },
                            Err(_) => Cell {
                                text: "Not valid UTF-8. Export the source as UTF-8 CSV.".into(),
                                kind: "error".into(),
                            },
                        };
                        ((i + 1) as u32, cell)
                    })
                    .collect();
                push(&mut sheet, SourceRow { row, cells }, &mut count, &mut text)?;
            }
            Ok(vec![sheet])
        }
        "ods" => ods(bytes, &mut count, &mut text),
        _ => Err("Choose XLSX, ODS, UTF-8 CSV or TSV. Save legacy XLS files as XLSX first.".into()),
    }
}

fn attrs(e: &quick_xml::events::BytesStart<'_>) -> Result<BTreeMap<String, String>, String> {
    e.attributes()
        .map(|a| {
            let a = a.map_err(|e| e.to_string())?;
            let value = std::str::from_utf8(&a.value).map_err(|e| e.to_string())?;
            Ok((
                String::from_utf8_lossy(a.key.local_name().as_ref()).to_string(),
                quick_xml::escape::unescape(value)
                    .map_err(|e| e.to_string())?
                    .into_owned(),
            ))
        })
        .collect()
}
fn repeats(a: &BTreeMap<String, String>, key: &str) -> Result<u32, String> {
    a.get(key)
        .map(|v| {
            v.parse::<u32>()
                .map_err(|_| "Invalid ODS repetition count.".into())
        })
        .unwrap_or(Ok(1))
        .and_then(|n| {
            if n == 0 {
                Err("Invalid ODS repetition count.".into())
            } else {
                Ok(n)
            }
        })
}
fn ods(bytes: &[u8], count: &mut usize, text: &mut usize) -> Result<Vec<Sheet>, String> {
    let mut zip = archive(bytes)?;
    let content = zip
        .by_name("content.xml")
        .map_err(|_| "ODS has no content.xml.")?;
    let mut xml = XmlReader::from_reader(std::io::BufReader::new(content));
    let mut buf = Vec::new();
    let mut sheets = Vec::new();
    let mut sheet: Option<Sheet> = None;
    let mut row = SourceRow {
        row: 0,
        cells: BTreeMap::new(),
    };
    let (mut row_n, mut row_repeat, mut col, mut col_repeat) = (1u32, 1u32, 1u32, 1u32);
    let mut cell: Option<Cell> = None;
    let mut paragraph = false;
    loop {
        let event = xml
            .read_event_into(&mut buf)
            .map_err(|e| format!("Malformed ODS: {e}"))?;
        let is_empty = matches!(&event, Event::Empty(_));
        match event {
            Event::DocType(_) => {
                return Err("Spreadsheet document declarations are not allowed.".into());
            }
            Event::Start(e) | Event::Empty(e) => {
                let a = attrs(&e)?;
                match e.local_name().as_ref() {
                    b"table" => {
                        if sheet.is_some() {
                            return Err("Nested ODS sheets are unsupported.".into());
                        }
                        if sheets.len() >= 128 {
                            return Err("Too many sheets.".into());
                        }
                        sheet = Some(Sheet {
                            name: a.get("name").cloned().unwrap_or_else(|| "Sheet".into()),
                            rows: Vec::new(),
                            error: None,
                        });
                        row_n = 1;
                    }
                    b"table-row" if sheet.is_some() => {
                        row = SourceRow {
                            row: row_n,
                            cells: BTreeMap::new(),
                        };
                        col = 1;
                        row_repeat = repeats(&a, "number-rows-repeated")?;
                    }
                    b"table-cell" | b"covered-table-cell" if sheet.is_some() => {
                        col_repeat = repeats(&a, "number-columns-repeated")?;
                        let kind = if a.contains_key("formula")
                            && !a.contains_key("value")
                            && !a.contains_key("string-value")
                        {
                            "error"
                        } else if a.get("value-type").is_some_and(|t| t == "date") {
                            "date"
                        } else {
                            "text"
                        };
                        let value = if kind == "error" {
                            "Formula has no cached value.".into()
                        } else {
                            a.get("date-value")
                                .or_else(|| a.get("boolean-value"))
                                .or_else(|| a.get("value"))
                                .or_else(|| a.get("string-value"))
                                .cloned()
                                .unwrap_or_default()
                        };
                        cell = Some(Cell {
                            kind: if kind == "text" && !value.is_empty() {
                                "scalar"
                            } else {
                                kind
                            }
                            .into(),
                            text: value,
                        });
                    }
                    b"p" if cell.is_some() => {
                        if cell
                            .as_ref()
                            .is_some_and(|c| c.kind == "text" && !c.text.is_empty())
                        {
                            cell.as_mut().unwrap().text.push('\n');
                        }
                        paragraph = true;
                    }
                    b"s" if paragraph => {
                        let n = repeats(&a, "c")?;
                        if n > 10_000 {
                            return Err("Too many ODS repeated spaces.".into());
                        }
                        if let Some(c) = &mut cell {
                            c.text.extend(std::iter::repeat_n(' ', n as usize));
                        }
                    }
                    b"tab" if paragraph => {
                        if let Some(c) = &mut cell {
                            c.text.push('\t');
                        }
                    }
                    b"line-break" if paragraph => {
                        if let Some(c) = &mut cell {
                            c.text.push('\n');
                        }
                    }
                    _ => {}
                }
                // Quick-xml Empty events have no corresponding End. Handle
                // them with the same finalizer below by looking at the source
                // event, not by expanding repeated blank cells into vectors.
                if is_empty {
                    finish_ods(
                        e.local_name().as_ref(),
                        &mut sheet,
                        &mut sheets,
                        &mut row,
                        &mut row_n,
                        row_repeat,
                        &mut col,
                        col_repeat,
                        &mut cell,
                        &mut paragraph,
                        count,
                        text,
                    )?;
                }
            }
            Event::Text(e) if paragraph && cell.as_ref().is_some_and(|c| c.kind == "text") => {
                let decoded = e.decode().map_err(|e| e.to_string())?;
                let value = quick_xml::escape::unescape(&decoded).map_err(|e| e.to_string())?;
                if let Some(c) = &mut cell {
                    c.text.push_str(&value);
                    if c.text.len() > 1024 * 1024 {
                        return Err("ODS cell text is too large.".into());
                    }
                }
            }
            Event::GeneralRef(e)
                if paragraph && cell.as_ref().is_some_and(|c| c.kind == "text") =>
            {
                let reference = format!("&{};", e.decode().map_err(|e| e.to_string())?);
                let decoded = quick_xml::escape::unescape(&reference).map_err(|e| e.to_string())?;
                if let Some(c) = &mut cell {
                    c.text.push_str(&decoded);
                }
            }
            Event::End(e) => finish_ods(
                e.local_name().as_ref(),
                &mut sheet,
                &mut sheets,
                &mut row,
                &mut row_n,
                row_repeat,
                &mut col,
                col_repeat,
                &mut cell,
                &mut paragraph,
                count,
                text,
            )?,
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if sheet.is_some() {
        return Err("ODS sheet is incomplete.".into());
    }
    Ok(sheets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn zipped(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, xml) in entries {
            z.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(xml.as_bytes()).unwrap();
        }
        z.finish().unwrap().into_inner()
    }
    #[test]
    fn delimited_preserves_text_and_physical_lines() {
        let s = parse(
            "a.csv",
            b"name,id,note\r\nA,001,\"line 1\nline 2\"\r\nB,002,ok\r\n",
            None,
        )
        .unwrap();
        assert_eq!(s[0].rows[1].cells[&2].text, "001");
        assert_eq!(s[0].rows[1].cells[&3].text, "line 1\nline 2");
        assert_eq!(s[0].rows[2].row, 4);
        let s = parse("a.tsv", b"Name\tID\nA\t001\n", None).unwrap();
        assert_eq!(s[0].rows[1].cells[&2].text, "001");
    }
    #[test]
    fn bad_utf8_is_a_cell_issue_not_whole_file_failure() {
        let s = parse("a.csv", b"Name\nOK\n\xff\nAgain\n", None).unwrap();
        assert_eq!(s[0].rows.len(), 4);
        assert_eq!(s[0].rows[2].cells[&1].kind, "error");
    }
    #[test]
    fn ods_sparse_repetition_and_typed_values() {
        let xml = r#"<office:document-content xmlns:office="urn:office" xmlns:table="urn:table" xmlns:text="urn:text"><table:table table:name="Sheet"><table:table-row><table:table-cell office:value-type="string"><text:p>Name</text:p></table:table-cell><table:table-cell office:value-type="string"><text:p>Number</text:p></table:table-cell></table:table-row><table:table-row><table:table-cell><text:p>A &amp; B</text:p><text:p>Next<text:s text:c="2"/>word</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="42"><text:p>42</text:p></table:table-cell><table:table-cell table:number-columns-repeated="100000"/></table:table-row><table:table-row table:number-rows-repeated="1000000"/></table:table></office:document-content>"#;
        let bytes = zipped(&[("content.xml", xml)]);
        let s = parse("a.ods", &bytes, None).unwrap();
        assert_eq!(s[0].rows.len(), 2);
        assert_eq!(s[0].rows[1].cells[&1].text, "A & B\nNext  word");
        assert_eq!(s[0].rows[1].cells[&2].text, "42");
    }
    #[test]
    fn excessive_ods_repeated_content_is_rejected() {
        let bytes = zipped(&[(
            "content.xml",
            r#"<table:table xmlns:table="urn:t" xmlns:text="urn:x"><table:table-row table:number-rows-repeated="4294967295"><table:table-cell><text:p>Nonblank</text:p></table:table-cell></table:table-row></table:table>"#,
        )]);
        assert!(
            parse("a.ods", &bytes, None)
                .unwrap_err()
                .contains("Too many")
        );
    }
    #[test]
    fn malformed_archive_and_legacy_xls_fail_clearly() {
        assert!(parse("a.xlsx", b"not a zip", None).is_err());
        assert!(
            parse("a.xls", b"", None)
                .unwrap_err()
                .contains("Save legacy XLS")
        );
    }
    #[test]
    fn malicious_shared_string_reservation_is_rejected() {
        let bytes = zipped(&[(
            "xl/sharedStrings.xml",
            r#"<sst uniqueCount="18446744073709551615"/>"#,
        )]);
        assert!(
            parse("a.xlsx", &bytes, None)
                .unwrap_err()
                .contains("shared-string")
        );
    }
    #[test]
    fn xlsx_streams_sparse_cells_and_cached_formulas() {
        let bytes = zipped(&[
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><workbookPr date1904="1"/><sheets><sheet name="First" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/styles.xml",
                r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><cellXfs count="3"><xf numFmtId="0"/><xf numFmtId="14"/><xf numFmtId="46"/></cellXfs></styleSheet>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:XFD1048576"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c></row><row r="2"><c r="A2"><f>1+1</f><v>2</v></c><c r="B2"><f>NA()</f></c><c r="C2" s="1"><v>0</v></c><c r="D2" s="1"><v>0.5</v></c><c r="E2" s="2"><v>0.5</v></c></row><row r="1048576"><c r="XFD1048576"/></row></sheetData></worksheet>"#,
            ),
        ]);
        let s = parse("a.xlsx", &bytes, None).unwrap();
        assert_eq!(s[0].rows.len(), 2);
        assert_eq!(s[0].rows[1].cells[&1].text, "2");
        assert_eq!(s[0].rows[1].cells[&2].kind, "error");
        assert_eq!(s[0].rows[1].cells[&3].text, "1904-01-01");
        assert_eq!(s[0].rows[1].cells[&4].text, "1904-01-01 12:00:00");
        assert_eq!(s[0].rows[1].cells[&5].text, "PT43200S");
    }
}
#[allow(clippy::too_many_arguments)]
fn finish_ods(
    tag: &[u8],
    sheet: &mut Option<Sheet>,
    sheets: &mut Vec<Sheet>,
    row: &mut SourceRow,
    row_n: &mut u32,
    row_repeat: u32,
    col: &mut u32,
    col_repeat: u32,
    cell: &mut Option<Cell>,
    paragraph: &mut bool,
    count: &mut usize,
    text: &mut usize,
) -> Result<(), String> {
    match tag {
        b"p" => *paragraph = false,
        b"table-cell" | b"covered-table-cell" => {
            if let Some(c) = cell.take().filter(|c| !c.text.trim().is_empty()) {
                if col_repeat > MAX_COLUMNS
                    || col
                        .checked_add(col_repeat)
                        .is_none_or(|n| n > MAX_COLUMNS + 1)
                {
                    return Err("ODS has too many populated columns.".into());
                }
                for i in 0..col_repeat {
                    row.cells.insert(*col + i, c.clone());
                }
            }
            *col = col.checked_add(col_repeat).ok_or("Invalid ODS column.")?;
        }
        b"table-row" => {
            if let Some(s) = sheet.as_mut() {
                if !row.cells.is_empty() {
                    if row_repeat as usize > MAX_ROWS {
                        return Err("Too many populated ODS rows.".into());
                    }
                    for i in 0..row_repeat {
                        let mut r = row.clone();
                        r.row = row_n.checked_add(i).ok_or("Invalid ODS row.")?;
                        push(s, r, count, text)?;
                    }
                }
                *row_n = row_n.checked_add(row_repeat).ok_or("Invalid ODS row.")?;
            }
        }
        b"table" => {
            if let Some(s) = sheet.take() {
                sheets.push(s);
            }
        }
        _ => {}
    }
    Ok(())
}
