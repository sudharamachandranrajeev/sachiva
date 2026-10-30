//! xlsx: a Summary sheet, then one sheet per section with its table, the
//! numbers behind each chart, and native spreadsheet charts drawn from them, so
//! the workbook can be checked and reused. Blank figures stay empty cells
//! and show as gaps in the charts.

use rust_xlsxwriter::{
    Chart, ChartFormat, ChartLegendPosition, ChartLine, ChartLineDashType, ChartMarker, ChartMarkerType,
    ChartSolidFill, ChartType, Color, DocProperties, Format, FormatAlign, FormatBorder, Workbook, Worksheet,
};
use sachiva_core::report::{self, Cell, ChartBody, Report};
use sachiva_core::spc::{Assurance, Point};

use crate::chart::{self, BARS};
use crate::{Error, SUMMARY_COLUMNS, fonts, summary_rows};

const CHART_ROWS: u32 = 20;

fn hex(c: &str) -> Color {
    Color::RGB(u32::from_str_radix(c.trim_start_matches('#'), 16).unwrap_or(0))
}

fn base() -> Format {
    Format::new().set_font_name(fonts::OFFICE).set_font_size(11)
}

fn header() -> Format {
    base().set_bold().set_background_color(hex("#d9d9d9")).set_border(FormatBorder::Thin).set_text_wrap()
}

/// Fill for a variation or assurance cell; the words carry the meaning too.
fn fill(colour: &str) -> Format {
    let text = if colour == chart::NEUTRAL { "#ffffff" } else { "#000000" };
    base().set_background_color(hex(colour)).set_font_color(hex(text)).set_border(FormatBorder::Thin).set_text_wrap()
}

/// Spreadsheet sheet names: at most 31 characters, none of []:*?/\ and unique.
fn sheet_name(i: usize, title: &str) -> String {
    let clean: String =
        title.chars().map(|c| if "[]:*?/\\'".contains(c) || c.is_control() { ' ' } else { c }).collect();
    let words = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut name = format!("{} {}", i + 1, words);
    if let Some((cut, _)) = name.char_indices().nth(31) {
        name.truncate(cut);
    }
    name.trim_end().to_string()
}

pub fn build(r: &Report) -> Result<Vec<u8>, Error> {
    let mut wb = Workbook::new();
    wb.set_properties(&DocProperties::new().set_title(&r.title).set_comment(&r.period));
    summary(wb.add_worksheet(), r)?;
    for (i, s) in r.sections.iter().enumerate() {
        let name = sheet_name(i, &s.title);
        let ws = wb.add_worksheet();
        ws.set_name(&name)?;
        section(ws, &name, s)?;
    }
    Ok(wb.save_to_buffer()?)
}

fn summary(ws: &mut Worksheet, r: &Report) -> Result<(), Error> {
    ws.set_name("Summary")?;
    ws.write_string_with_format(0, 0, &r.title, &base().set_bold().set_font_size(16))?;
    ws.write_string_with_format(1, 0, &r.period, &base())?;
    ws.write_string_with_format(2, 0, format!("Made {}", r.made), &base())?;
    for (c, h) in SUMMARY_COLUMNS.iter().enumerate() {
        ws.write_string_with_format(4, c as u16, *h, &header())?;
    }
    let cell = base().set_border(FormatBorder::Thin).set_text_wrap();
    let number = base().set_border(FormatBorder::Thin).set_align(FormatAlign::Right);
    for (i, (row, text)) in r.summary.iter().zip(summary_rows(r)).enumerate() {
        let y = 5 + i as u32;
        ws.write_string_with_format(y, 0, &text[0], &cell)?;
        for (c, v) in [(1, row.latest), (2, row.target)] {
            match v {
                Some(v) => ws.write_number_with_format(y, c, v, &number)?,
                None => ws.write_blank(y, c, &number)?,
            };
        }
        let var = match row.variation.map(|v| v.point) {
            Some(p) => fill(chart::point_colour(p)),
            None => cell.clone(),
        };
        ws.write_string_with_format(y, 3, &text[3], &var)?;
        let ass = match row.assurance {
            Some(Assurance::Pass) => fill(chart::IMPROVEMENT),
            Some(Assurance::Fail) => fill(chart::CONCERN),
            Some(Assurance::Inconsistent) => fill(chart::COMMON),
            None => cell.clone(),
        };
        ws.write_string_with_format(y, 4, &text[4], &ass)?;
        ws.write_string_with_format(y, 5, &text[5], &cell)?;
    }
    let end = 5 + r.summary.len() as u32 + 1;
    ws.write_string_with_format(end, 0, &r.footnote, &base().set_italic())?;
    ws.set_freeze_panes(5, 0)?;
    for (c, w) in [(0, 48), (1, 10), (2, 10), (3, 24), (4, 30), (5, 22)] {
        ws.set_column_width(c, w)?;
    }
    Ok(())
}

fn write_cell(ws: &mut Worksheet, y: u32, x: u16, c: &Cell, f: &Format) -> Result<(), Error> {
    match c {
        Cell::Text(s) => ws.write_string_with_format(y, x, s, f)?,
        Cell::Num(Some(v)) => ws.write_number_with_format(y, x, *v, f)?,
        Cell::Num(None) => ws.write_blank(y, x, f)?,
    };
    Ok(())
}

fn section(ws: &mut Worksheet, name: &str, s: &report::Section) -> Result<(), Error> {
    ws.write_string_with_format(0, 0, &s.title, &base().set_bold().set_font_size(14))?;
    if !s.intro.is_empty() {
        ws.write_string_with_format(1, 0, &s.intro, &base())?;
    }
    let cols = s.table.columns.len() as u16;
    for (c, h) in s.table.columns.iter().enumerate() {
        ws.write_string_with_format(3, c as u16, h, &header())?;
    }
    let cell = base().set_border(FormatBorder::Thin);
    let decimal = cell.clone().set_num_format("0.0");
    for (i, row) in s.table.rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            write_cell(ws, 4 + i as u32, c as u16, v, &cell)?;
        }
    }
    ws.set_freeze_panes(4, 1)?;
    ws.set_column_width(0, 18)?;
    for c in 1..cols.max(10) {
        ws.set_column_width(c, 13)?;
    }

    // The numbers behind each chart, below the table; the charts sit beside it.
    let mut y = 4 + s.table.rows.len() as u32 + 2;
    for (k, c) in s.charts.iter().enumerate() {
        ws.write_string_with_format(y, 0, format!("Chart data: {}", c.title), &base().set_bold())?;
        let head = y + 1;
        let first = head + 1;
        let last = first + c.labels.len().max(1) as u32 - 1;
        ws.write_string_with_format(head, 0, "Period", &header())?;
        for (i, l) in c.labels.iter().enumerate() {
            ws.write_string_with_format(first + i as u32, 0, l, &cell)?;
        }
        let column = |ws: &mut Worksheet, x: u16, title: &str, vals: &[Option<f64>], f: &Format| -> Result<(), Error> {
            ws.write_string_with_format(head, x, title, &header())?;
            for (i, v) in vals.iter().enumerate() {
                write_cell(ws, first + i as u32, x, &Cell::Num(*v), f)?;
            }
            Ok(())
        };
        let n = c.labels.len();
        let range = |x: u16| (name, first, x, last, x);
        let mut ch;
        match &c.body {
            ChartBody::Spc { values, spc, target, .. } => {
                ch = Chart::new(ChartType::Line);
                column(ws, 1, "Value", values, &cell)?;
                let line = |ch: &mut Chart, x: u16, title: &str, colour: &str, dash: bool| {
                    let mut l = ChartLine::new();
                    l.set_color(hex(colour)).set_width(1.5);
                    if dash {
                        l.set_dash_type(ChartLineDashType::Dash);
                    }
                    ch.add_series()
                        .set_name(title)
                        .set_categories(range(0))
                        .set_values(range(x))
                        .set_format(ChartFormat::new().set_line(&l))
                        .set_marker(ChartMarker::new().set_none());
                };
                ch.add_series()
                    .set_name("Value")
                    .set_categories(range(0))
                    .set_values(range(1))
                    .set_format(
                        ChartFormat::new().set_line(ChartLine::new().set_color(hex(chart::COMMON)).set_width(2)),
                    )
                    .set_marker(&marker(chart::COMMON));
                let mut x = 2;
                if let Some(spc) = spc {
                    for (title, v, colour, dash) in [
                        ("Average", spc.mean, chart::MEAN, false),
                        ("Upper process limit", spc.upper, chart::LIMITS, true),
                        ("Lower process limit", spc.lower, chart::LIMITS, true),
                    ] {
                        column(ws, x, title, &vec![Some(v); n], &decimal)?;
                        line(&mut ch, x, title, colour, dash);
                        x += 1;
                    }
                }
                if let Some(t) = target {
                    column(ws, x, "Target", &vec![Some(*t); n], &cell)?;
                    line(&mut ch, x, "Target", chart::TARGET, true);
                    x += 1;
                }
                // Special-cause points as their own marker-only series.
                if let Some(spc) = spc {
                    for (p, title) in
                        [(Point::Improvement, "Improvement"), (Point::Concern, "Concern"), (Point::Neutral, "Change")]
                    {
                        let only: Vec<Option<f64>> = values
                            .iter()
                            .zip(&spc.points)
                            .map(|(v, q)| if *q == Some(p) { *v } else { None })
                            .collect();
                        if only.iter().any(Option::is_some) {
                            column(ws, x, title, &only, &cell)?;
                            ch.add_series()
                                .set_name(title)
                                .set_categories(range(0))
                                .set_values(range(x))
                                .set_format(ChartFormat::new().set_no_line())
                                .set_marker(&marker(chart::point_colour(p)));
                            x += 1;
                        }
                    }
                }
                if values.iter().flatten().all(|v| *v >= 0.0) {
                    ch.y_axis().set_min(0);
                }
            }
            ChartBody::Bars { series, stacked } => {
                ch = Chart::new(if *stacked { ChartType::ColumnStacked } else { ChartType::Column });
                for (j, sr) in series.iter().enumerate() {
                    let x = 1 + j as u16;
                    column(ws, x, &sr.name, &sr.values, &cell)?;
                    ch.add_series()
                        .set_name(sr.name.as_str())
                        .set_categories(range(0))
                        .set_values(range(x))
                        .set_format(
                            ChartFormat::new()
                                .set_solid_fill(ChartSolidFill::new().set_color(hex(BARS[j % BARS.len()])))
                                .set_border(ChartLine::new().set_color(hex("#333333"))),
                        );
                }
            }
        }
        ch.title().set_name(c.title.as_str());
        ch.y_axis().set_name(c.y_label.as_str());
        ch.legend().set_position(ChartLegendPosition::Bottom);
        ch.set_alt_text(c.alt.as_str());
        ch.set_width(720).set_height(360);
        ws.insert_chart(3 + k as u32 * CHART_ROWS, cols + 1, &ch)?;
        y = last + 3;
    }
    Ok(())
}

fn marker(colour: &str) -> ChartMarker {
    let mut m = ChartMarker::new();
    m.set_type(ChartMarkerType::Circle).set_size(7).set_format(
        ChartFormat::new()
            .set_solid_fill(ChartSolidFill::new().set_color(hex(colour)))
            .set_border(ChartLine::new().set_color(hex(colour))),
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_names_are_safe() {
        assert_eq!(sheet_name(0, "A/B: [x]"), "1 A B x");
        let long = sheet_name(11, &"word ".repeat(20));
        assert!(long.chars().count() <= 31 && long.starts_with("12 word"));
    }
}
