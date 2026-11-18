//! Individual format decoders/encoders plus format auto-detection.

pub mod bufkit;
pub mod csv;
pub mod iem;
pub mod spc;
pub mod uwyo;

use crate::Format;

/// Guess the format of `text` from its content. Returns `None` if nothing matches.
pub fn detect_format(text: &str) -> Option<Format> {
    let head: String = text.chars().take(20_000).collect();
    let trimmed = head.trim_start_matches('\u{feff}').trim_start();
    if head.contains("%RAW%") || head.contains("%TITLE%") {
        return Some(Format::Spc);
    }
    if head.contains("SNPARM") || (has_line_starting(&head, "STID") && head.contains("STIM")) {
        return Some(Format::Bufkit);
    }
    if (trimmed.starts_with('{') || trimmed.starts_with('[')) && head.contains("\"profile") {
        return Some(Format::IemJson);
    }
    let up = head.to_ascii_uppercase();
    if up.contains("<PRE>")
        || up.contains("UNIVERSITY OF WYOMING")
        || up.contains("WYOMING WEATHER WEB")
        || uwyo::find_table_header(&head).is_some()
    {
        return Some(Format::Uwyo);
    }
    if csv::looks_like_csv(&head) {
        return Some(Format::Csv);
    }
    None
}

fn has_line_starting(text: &str, prefix: &str) -> bool {
    text.lines().any(|l| l.trim_start().starts_with(prefix))
}

/// Logical sounding columns that decoders map source headers onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Col {
    Pres,
    Hght,
    Tmpc,
    Dwpc,
    Wdir,
    Wspd,
    Omeg,
}

/// Map a source column name (case-insensitive) to a logical column, for names that
/// appear across SPC/GEMPAK/BUFKIT/UWyo/IEM conventions. Unit-bearing variants are
/// handled by the individual decoders.
pub(crate) fn column_for(name: &str) -> Option<Col> {
    Some(match name.trim().to_ascii_uppercase().as_str() {
        "PRES" | "LEVEL" | "PRESSURE" | "P" => Col::Pres,
        "HGHT" | "HEIGHT" | "Z" | "GPH" => Col::Hght,
        "TEMP" | "TMPC" | "T" | "TEMPERATURE" => Col::Tmpc,
        "DWPT" | "DWPC" | "DEWP" | "TD" | "DEWPOINT" => Col::Dwpc,
        "WDIR" | "DRCT" | "DIR" => Col::Wdir,
        "WSPD" | "SKNT" | "SPEED" => Col::Wspd,
        "OMEG" | "OMGA" | "OMEGA" => Col::Omeg,
        _ => return None,
    })
}

/// Split `KEY = VALUE KEY2 = VALUE2` (GEMPAK/BUFKIT header style) into pairs. Handles
/// empty values (`STID = STNM = 724350`) and glued `KEY=VALUE`.
pub(crate) fn key_values(line: &str) -> Vec<(String, String)> {
    let spaced = line.replace('=', " = ");
    let toks: Vec<&str> = spaced.split_whitespace().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < toks.len() {
        if toks[i] != "=" && toks[i + 1] == "=" {
            let key = toks[i].to_string();
            let val = match toks.get(i + 2) {
                Some(v) if *v != "=" && toks.get(i + 3) != Some(&"=") => {
                    i += 3;
                    v.to_string()
                }
                _ => {
                    i += 2;
                    String::new()
                }
            };
            out.push((key, val));
        } else {
            i += 1;
        }
    }
    out
}
