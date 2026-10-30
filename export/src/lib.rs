//! Turns a Sachiva report into a file: a spreadsheet (.xlsx), document (.docx), slides (.pptx) or PDF.
//! Every format draws from the same `Report`, so they always agree. Files
//! are built in memory; the app writes them where the user chose.

pub mod chart;
mod docx;
mod ooxml;
mod pdf;
mod pptx;
mod xlsx;

use sachiva_core::report::{Cell, Report};
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not draw a chart: {0}")]
    Chart(String),
    #[error("could not make the xlsx file: {0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),
    #[error("could not make the docx file: {0}")]
    Docx(String),
    #[error("could not make the pptx file: {0}")]
    Pptx(String),
    #[error("could not make the PDF: {0}")]
    Pdf(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Format {
    Xlsx,
    Docx,
    Pptx,
    Pdf,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Xlsx => "xlsx",
            Format::Docx => "docx",
            Format::Pptx => "pptx",
            Format::Pdf => "pdf",
        }
    }
}

pub fn export(report: &Report, format: Format) -> Result<Vec<u8>, Error> {
    match format {
        Format::Xlsx => xlsx::build(report),
        Format::Docx => docx::build(report),
        Format::Pptx => pptx::build(report),
        Format::Pdf => pdf::build(report),
    }
}

/// Liberation Sans: Arial's metrics under the SIL Open Font License, so
/// charts and PDFs look the same on every machine.
pub(crate) mod fonts {
    pub const REGULAR: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");
    pub const BOLD: &[u8] = include_bytes!("../fonts/LiberationSans-Bold.ttf");
    /// What Office files ask for; Liberation Sans stands in where it is missing.
    pub const OFFICE: &str = "Arial";

    pub fn database() -> resvg::usvg::fontdb::Database {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_font_data(REGULAR.to_vec());
        db.load_font_data(BOLD.to_vec());
        db.set_sans_serif_family("Liberation Sans");
        db
    }
}

/// A number as shown in tables: whole numbers plain, others to 1 place.
pub(crate) fn num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

/// Table text for a cell; a blank figure is a dash ("not entered").
pub(crate) fn cell_text(c: &Cell) -> String {
    match c {
        Cell::Text(s) => s.clone(),
        Cell::Num(Some(v)) => num(*v),
        Cell::Num(None) => "–".into(),
    }
}

/// The summary table as text rows, shared by docx, pptx, PDF and
/// the on-screen preview.
pub const SUMMARY_COLUMNS: [&str; 6] = ["Measure", "Latest", "Target", "Variation", "Assurance", "Target met"];

pub fn summary_rows(report: &Report) -> Vec<[String; 6]> {
    report
        .summary
        .iter()
        .map(|r| {
            [
                r.measure.clone(),
                r.latest.map(num).unwrap_or_else(|| "–".into()),
                r.target.map(num).unwrap_or_default(),
                r.variation.map(|v| v.words().to_string()).unwrap_or_else(|| "Too few figures".into()),
                r.assurance.map(|a| a.words().to_string()).unwrap_or_default(),
                r.met.clone(),
            ]
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod sample {
    use sachiva_core::jiff::civil::date;
    use sachiva_core::jiff::tz::TimeZone;
    use sachiva_core::model::Span;
    use sachiva_core::report::{self, Report, ReportKind, Request};
    use sachiva_core::waitlist::{Figures, WaitList};

    /// A synthetic waiting-list report with two lists, a blank month and
    /// a name that needs escaping in XML.
    pub fn report() -> Report {
        let tz = TimeZone::get("Europe/London").unwrap();
        let mut a = WaitList::new("Follow-ups <A & B>");
        a.target_weeks = Some(52);
        let b = WaitList::new("New patients");
        let mut figs = Vec::new();
        for m in 1..=8 {
            let start = date(2026, m, 1);
            let mut fa = Figures::empty(&a.id, Span::Month, start, 4);
            fa.total = Some(100 + u32::from(m as u8) * 3);
            fa.added = Some(20);
            fa.removed = Some(15);
            fa.booked = Some(10);
            fa.bands = vec![Some(40), Some(30), Some(20), Some(10)];
            fa.longest_weeks = Some(50 + u32::from(m as u8));
            figs.push(fa);
            if m != 4 {
                let mut fb = Figures::empty(&b.id, Span::Month, start, 4);
                fb.total = Some(60);
                figs.push(fb);
            }
        }
        let req = Request {
            kind: ReportKind::WaitingList,
            span: Span::Month,
            from: date(2026, 1, 1),
            to: date(2026, 8, 31),
            activities: vec![],
            lists: vec![],
        };
        report::waiting(&req, &[a, b], &figs, &tz, "2026-09-02T10:00:00Z".parse().unwrap())
    }
}

#[cfg(test)]
mod tests {
    /// Writes every format to `$SACHIVA_EXPORT_DUMP` for a look by eye.
    #[test]
    #[ignore]
    fn dump() {
        let dir = std::env::var("SACHIVA_EXPORT_DUMP").expect("set SACHIVA_EXPORT_DUMP");
        let r = crate::sample::report();
        for f in [super::Format::Xlsx, super::Format::Docx, super::Format::Pptx, super::Format::Pdf] {
            let bytes = super::export(&r, f).unwrap();
            std::fs::write(format!("{dir}/sample.{}", f.extension()), bytes).unwrap();
        }
    }
}
