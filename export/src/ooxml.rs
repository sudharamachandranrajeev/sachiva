//! Small helpers for the hand-written Office Open XML packages (docx and
//! pptx): XML escaping, the zip container and document properties.
//! Writing the XML ourselves lets every picture carry its alt text and
//! every heading and table header be marked as such.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

pub const EMU_PER_INCH: i64 = 914_400;

/// Escapes text for XML content and attributes, dropping characters XML
/// cannot hold at all.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(' '),
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
    out
}

pub struct Package {
    zip: ZipWriter<Cursor<Vec<u8>>>,
}

impl Package {
    pub fn new() -> Package {
        Package { zip: ZipWriter::new(Cursor::new(Vec::new())) }
    }

    /// Adds a part. A fixed timestamp keeps the same report byte-for-byte the same.
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(DateTime::default());
        self.zip.start_file(name, opts).map_err(|e| e.to_string())?;
        self.zip.write_all(data).map_err(|e| e.to_string())
    }

    pub fn finish(self) -> Result<Vec<u8>, String> {
        Ok(self.zip.finish().map_err(|e| e.to_string())?.into_inner())
    }
}

/// `docProps/core.xml`: the title only. No author, so no name leaks in.
pub fn core_props(title: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title><dc:language>en-GB</dc:language></cp:coreProperties>"#,
        esc(title)
    )
}

/// Hex colour without the `#`, as Office wants it.
pub fn rgb(c: &str) -> String {
    c.trim_start_matches('#').to_uppercase()
}

#[cfg(test)]
pub mod check {
    use std::io::Read;

    /// Every part of a package, read back, with each XML part parsed.
    pub fn parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut out = Vec::new();
        for i in 0..z.len() {
            let mut f = z.by_index(i).unwrap();
            let mut data = Vec::new();
            f.read_to_end(&mut data).unwrap();
            let name = f.name().to_string();
            if name.ends_with(".xml") || name.ends_with(".rels") {
                let text = std::str::from_utf8(&data).unwrap();
                roxmltree::Document::parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
            }
            out.push((name, data));
        }
        out
    }

    pub fn text<'a>(parts: &'a [(String, Vec<u8>)], name: &str) -> &'a str {
        let p = parts.iter().find(|p| p.0 == name).unwrap_or_else(|| panic!("no {name}"));
        std::str::from_utf8(&p.1).unwrap()
    }
}
