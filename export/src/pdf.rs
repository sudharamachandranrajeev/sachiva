//! PDF: A4 landscape, tagged to PDF/UA-1 (headings, paragraphs, tables
//! with header cells, charts as figures with alt text), with the charts as
//! vector graphics and the bundled font embedded.
//!
//! Laid out in two passes: first into pages of drawing items, each marked
//! with the logical element it belongs to; then drawn, collecting the tag
//! identifiers to build the structure tree in reading order.

use std::num::NonZeroU16;

use krilla::color::rgb;
use krilla::configure::{Accessibility, ConfigurationBuilder, Validator};
use krilla::destination::XyzDestination;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::metadata::Metadata;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::surface::Surface;
use krilla::tagging::{
    Artifact, ArtifactType, ContentTag, Identifier, SpanTag, TableHeaderScope, Tag, TagGroup, TagTree,
};
use krilla::text::{Font, TextDirection};
use krilla::{Document, SerializeSettings};
use krilla_svg::{SurfaceExt, SvgSettings};
use resvg::usvg;
use sachiva_core::report::Report;
use sachiva_core::spc::Assurance;

use crate::chart;
use crate::{Error, SUMMARY_COLUMNS, cell_text, fonts, summary_rows};

const PW: f32 = 842.0;
const PH: f32 = 595.0;
const M: f32 = 40.0;
const FOOT: f32 = 24.0;
const BODY: f32 = 11.0;
const CELL: f32 = 9.5;
const PAD: f32 = 4.0;
const INK: &str = "#1a1a1a";

fn colour(hex: &str) -> rgb::Color {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    rgb::Color::new((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Advance widths from the bundled font, for wrapping and fitting columns.
struct Metrics {
    regular: ttf_parser::Face<'static>,
    bold: ttf_parser::Face<'static>,
}

impl Metrics {
    fn new() -> Result<Metrics, Error> {
        let parse = |d| ttf_parser::Face::parse(d, 0).map_err(|e| Error::Pdf(e.to_string()));
        Ok(Metrics { regular: parse(fonts::REGULAR)?, bold: parse(fonts::BOLD)? })
    }

    fn width(&self, s: &str, size: f32, bold: bool) -> f32 {
        let f = if bold { &self.bold } else { &self.regular };
        let units: u32 =
            s.chars().map(|c| f.glyph_index(c).and_then(|g| f.glyph_hor_advance(g)).map_or(500, u32::from)).sum();
        units as f32 * size / f32::from(f.units_per_em())
    }

    /// Breaks text into lines no wider than `max`, splitting long words too.
    fn wrap(&self, text: &str, size: f32, bold: bool, max: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in text.split_whitespace() {
            let tried = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if self.width(&tried, size, bold) <= max {
                line = tried;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            for c in word.chars() {
                line.push(c);
                if self.width(&line, size, bold) > max && line.chars().count() > 1 {
                    let last = line.pop().unwrap_or(c);
                    lines.push(std::mem::take(&mut line));
                    line.push(last);
                }
            }
        }
        if !line.is_empty() || lines.is_empty() {
            lines.push(line);
        }
        lines
    }
}

enum Draw {
    Text { x: f32, y: f32, size: f32, bold: bool, colour: &'static str, text: String },
    Rect { x: f32, y: f32, w: f32, h: f32, fill: Option<&'static str>, stroke: bool },
    Svg { tree: Box<usvg::Tree>, x: f32, y: f32 },
}

/// Which logical element an item belongs to; `None` = decoration.
struct Item {
    slot: Option<usize>,
    draw: Draw,
}

enum Logical {
    Heading(u16, String, usize),
    P(usize),
    Figure(String, usize),
    /// Rows of (header cell?, slot).
    Table(Vec<Vec<(bool, usize)>>),
}

struct Layout {
    m: Metrics,
    pages: Vec<Vec<Item>>,
    y: f32,
    slots: usize,
    tree: Vec<Logical>,
    /// Headings for the bookmarks: level, text, page, top.
    marks: Vec<(u16, String, usize, f32)>,
}

impl Layout {
    fn slot(&mut self) -> usize {
        self.slots += 1;
        self.slots - 1
    }

    fn page(&mut self) {
        self.pages.push(Vec::new());
        self.y = M;
    }

    fn room(&self) -> f32 {
        PH - M - FOOT - self.y
    }

    fn need(&mut self, h: f32) {
        if h > self.room() && self.y > M {
            self.page();
        }
    }

    fn push(&mut self, slot: Option<usize>, draw: Draw) {
        if let Some(p) = self.pages.last_mut() {
            p.push(Item { slot, draw });
        }
    }

    fn lines(&mut self, slot: usize, text: &str, size: f32, bold: bool, colour: &'static str, keep: f32) {
        let lh = size * 1.3;
        let lines = self.m.wrap(text, size, bold, PW - 2.0 * M);
        self.need(lh * lines.len().min(3) as f32 + keep);
        for l in lines {
            self.need(lh);
            self.push(Some(slot), Draw::Text { x: M, y: self.y + size, size, bold, colour, text: l });
            self.y += lh;
        }
    }

    /// `keep`: room the next block needs, so a heading never ends a page.
    fn heading(&mut self, level: u16, text: &str, keep: f32) {
        let size = match level {
            1 => 18.0,
            _ => 13.0,
        };
        self.y += size * 0.6;
        let s = self.slot();
        self.lines(s, text, size, true, INK, keep);
        let top = self.y - size * 1.3;
        self.marks.push((level, text.to_string(), self.pages.len() - 1, top));
        self.y += 2.0;
        self.tree.push(Logical::Heading(level, text.to_string(), s));
    }

    fn para(&mut self, text: &str, size: f32, colour: &'static str) {
        let s = self.slot();
        self.lines(s, text, size, false, colour, 0.0);
        self.y += size * 0.5;
        self.tree.push(Logical::P(s));
    }

    fn figure(&mut self, c: &sachiva_core::report::Chart) -> Result<(), Error> {
        let tree = chart::tree(&chart::svg(c))?;
        self.need(chart::H);
        let s = self.slot();
        self.push(Some(s), Draw::Svg { tree: Box::new(tree), x: M + (PW - 2.0 * M - chart::W) / 2.0, y: self.y });
        self.y += chart::H + 8.0;
        self.tree.push(Logical::Figure(c.alt.clone(), s));
        Ok(())
    }

    fn table(&mut self, head: &[String], rows: &[Vec<String>], fills: &[Vec<Option<(&'static str, &'static str)>>]) {
        let avail = PW - 2.0 * M;
        let natural: Vec<f32> = (0..head.len())
            .map(|c| {
                let cells = rows.iter().filter_map(|r| r.get(c)).map(|t| self.m.width(t, CELL, false));
                cells.fold(self.m.width(&head[c], CELL, true), f32::max).clamp(30.0, 260.0) + 2.0 * PAD
            })
            .collect();
        let sum: f32 = natural.iter().sum();
        let widths: Vec<f32> = natural.iter().map(|w| w * avail / sum).collect();
        let lh = CELL * 1.3;

        let wrap_row = |m: &Metrics, cells: &[String], bold: bool| -> Vec<Vec<String>> {
            cells.iter().zip(&widths).map(|(t, w)| m.wrap(t, CELL, bold, w - 2.0 * PAD)).collect()
        };
        let head_lines = wrap_row(&self.m, head, true);
        let head_h = head_lines.iter().map(Vec::len).max().unwrap_or(1) as f32 * lh + 2.0 * PAD;

        // Draws one row; `slots` None = a repeated header, marked as decoration.
        let draw_row = |l: &mut Layout,
                        lines: &[Vec<String>],
                        h: f32,
                        bold: bool,
                        fill: &[Option<(&'static str, &'static str)>],
                        slots: Option<&[usize]>| {
            let mut x = M;
            for (c, cell) in lines.iter().enumerate() {
                let (bg, fg) = match fill.get(c).copied().flatten() {
                    Some((bg, fg)) => (Some(bg), fg),
                    None => (bold.then_some("#d9d9d9"), INK),
                };
                l.push(None, Draw::Rect { x, y: l.y, w: widths[c], h, fill: bg, stroke: true });
                for (i, t) in cell.iter().enumerate() {
                    let draw = Draw::Text {
                        x: x + PAD,
                        y: l.y + PAD + CELL + i as f32 * lh,
                        size: CELL,
                        bold,
                        colour: fg,
                        text: t.clone(),
                    };
                    l.push(slots.map(|s| s[c]), draw);
                }
                x += widths[c];
            }
            l.y += h;
        };

        self.need(head_h + lh + 2.0 * PAD);
        let mut logical = Vec::new();
        let hs: Vec<usize> = head.iter().map(|_| self.slot()).collect();
        draw_row(self, &head_lines, head_h, true, &[], Some(&hs));
        logical.push(hs.iter().map(|&s| (true, s)).collect());
        for (i, r) in rows.iter().enumerate() {
            let lines = wrap_row(&self.m, r, false);
            let h = lines.iter().map(Vec::len).max().unwrap_or(1) as f32 * lh + 2.0 * PAD;
            if h > self.room() {
                self.page();
                draw_row(self, &head_lines, head_h, true, &[], None);
            }
            let ss: Vec<usize> = r.iter().map(|_| self.slot()).collect();
            draw_row(self, &lines, h, false, fills.get(i).map_or(&[][..], |f| &f[..]), Some(&ss));
            logical.push(ss.iter().map(|&s| (false, s)).collect());
        }
        self.y += 10.0;
        self.tree.push(Logical::Table(logical));
    }
}

fn shade(bg: &'static str) -> (&'static str, &'static str) {
    (bg, if bg == chart::NEUTRAL { "#ffffff" } else { "#000000" })
}

pub fn build(r: &Report) -> Result<Vec<u8>, Error> {
    let mut l = Layout { m: Metrics::new()?, pages: Vec::new(), y: M, slots: 0, tree: Vec::new(), marks: Vec::new() };
    l.page();
    l.heading(1, &r.title, 0.0);
    l.para(&r.period, BODY, INK);
    l.para(&format!("Made {}", r.made), BODY, INK);

    l.heading(1, "Summary", 60.0);
    let head: Vec<String> = SUMMARY_COLUMNS.iter().map(|s| s.to_string()).collect();
    let rows: Vec<Vec<String>> = summary_rows(r).into_iter().map(Vec::from).collect();
    let fills: Vec<Vec<Option<(&str, &str)>>> = r
        .summary
        .iter()
        .map(|s| {
            let var = s.variation.map(|v| shade(chart::point_colour(v.point)));
            let ass = s.assurance.map(|a| {
                shade(match a {
                    Assurance::Pass => chart::IMPROVEMENT,
                    Assurance::Fail => chart::CONCERN,
                    Assurance::Inconsistent => chart::COMMON,
                })
            });
            vec![None, None, None, var, ass, None]
        })
        .collect();
    if rows.is_empty() {
        l.para("Nothing to show for this period.", BODY, INK);
    } else {
        l.table(&head, &rows, &fills);
    }

    for s in &r.sections {
        // Keep the section heading with its first chart.
        l.heading(1, &s.title, if s.charts.is_empty() { 60.0 } else { chart::H + 40.0 });
        if !s.intro.is_empty() {
            l.para(&s.intro, BODY, INK);
        }
        for c in &s.charts {
            l.heading(2, &c.title, chart::H);
            l.figure(c)?;
        }
        l.heading(2, "Figures", 60.0);
        let rows: Vec<Vec<String>> = s.table.rows.iter().map(|row| row.iter().map(cell_text).collect()).collect();
        l.table(&s.table.columns, &rows, &[]);
    }
    l.para(&r.footnote, 9.0, "#404040");
    render(r, l)
}

fn render(r: &Report, l: Layout) -> Result<Vec<u8>, Error> {
    let config = ConfigurationBuilder::new()
        .set_validator(Validator::Ua(Accessibility::UA1))
        .finish()
        .map_err(|e| Error::Pdf(format!("{e:?}")))?;
    let mut doc = Document::new_with(SerializeSettings { configuration: config, ..SerializeSettings::default() });
    let regular = Font::new(fonts::REGULAR.into(), 0).ok_or_else(|| Error::Pdf("font".into()))?;
    let bold = Font::new(fonts::BOLD.into(), 0).ok_or_else(|| Error::Pdf("font".into()))?;
    let mut ids: Vec<Vec<Identifier>> = (0..l.slots).map(|_| Vec::new()).collect();
    let total = l.pages.len();

    for (n, items) in l.pages.iter().enumerate() {
        let settings = PageSettings::from_wh(PW, PH).ok_or_else(|| Error::Pdf("page size".into()))?;
        let mut page = doc.start_page_with(settings);
        let mut s = page.surface();
        for it in items {
            let tag = match (&it.draw, it.slot) {
                (_, None) => ContentTag::Artifact(Artifact::with_kind(ArtifactType::Layout)),
                (Draw::Svg { .. }, Some(_)) => ContentTag::Other,
                (_, Some(_)) => ContentTag::Span(SpanTag::empty()),
            };
            let id = s.start_tagged(tag);
            draw(&mut s, &it.draw, &regular, &bold)?;
            s.end_tagged();
            if let Some(k) = it.slot {
                ids[k].push(id);
            }
        }
        let foot = format!("Page {} of {total}", n + 1);
        let w = l.m.width(&foot, 9.0, false);
        s.start_tagged(ContentTag::Artifact(Artifact::with_kind(ArtifactType::PageNumber)));
        let text = Draw::Text { x: PW - M - w, y: PH - M / 2.0, size: 9.0, bold: false, colour: "#404040", text: foot };
        draw(&mut s, &text, &regular, &bold)?;
        s.end_tagged();
        s.finish();
        page.finish();
    }

    let mut ids: Vec<Option<Vec<Identifier>>> = ids.into_iter().map(Some).collect();
    let mut take = |k: usize| -> Vec<krilla::tagging::Node> {
        ids[k].take().unwrap_or_default().into_iter().map(Into::into).collect()
    };
    let mut tree = TagTree::new().with_lang(Some("en-GB".into()));
    for node in &l.tree {
        let group = match node {
            Logical::Heading(level, text, k) => {
                let level = NonZeroU16::new(*level).unwrap_or(NonZeroU16::MIN);
                TagGroup::with_children(Tag::Hn(level, Some(text.clone())), take(*k))
            }
            Logical::P(k) => TagGroup::with_children(Tag::P, take(*k)),
            Logical::Figure(alt, k) => TagGroup::with_children(Tag::Figure(Some(alt.clone())), take(*k)),
            Logical::Table(rows) => {
                let mut t = TagGroup::new(Tag::Table);
                for row in rows {
                    let mut tr = TagGroup::new(Tag::TR);
                    for &(head, k) in row {
                        let cell = if head {
                            TagGroup::with_children(Tag::TH(TableHeaderScope::Column), take(k))
                        } else {
                            TagGroup::with_children(Tag::TD, take(k))
                        };
                        tr.push(cell);
                    }
                    t.push(tr);
                }
                t
            }
        };
        tree.push(group);
    }
    doc.set_tag_tree(tree);
    let mut outline = Outline::new();
    let mut open: Option<OutlineNode> = None;
    for (level, text, page, y) in &l.marks {
        let node = OutlineNode::new(text.clone(), XyzDestination::new(*page, Point::from_xy(M, *y)));
        if *level == 1 {
            if let Some(done) = open.replace(node) {
                outline.push_child(done);
            }
        } else if let Some(parent) = open.as_mut() {
            parent.push_child(node);
        } else {
            outline.push_child(node);
        }
    }
    if let Some(done) = open {
        outline.push_child(done);
    }
    doc.set_outline(outline);
    doc.set_metadata(Metadata::new().title(r.title.clone()).language("en-GB".into()).creator("Sachiva".into()));
    doc.finish().map_err(|e| Error::Pdf(format!("{e:?}")))
}

fn draw(s: &mut Surface, d: &Draw, regular: &Font, bold: &Font) -> Result<(), Error> {
    match d {
        Draw::Text { x, y, size, bold: b, colour: c, text } => {
            s.set_fill(Some(Fill { paint: colour(c).into(), ..Fill::default() }));
            s.set_stroke(None);
            let font = if *b { bold } else { regular };
            s.draw_text(Point::from_xy(*x, *y), font.clone(), *size, text, false, TextDirection::Auto);
        }
        Draw::Rect { x, y, w, h, fill, stroke } => {
            let mut p = PathBuilder::new();
            if let Some(rect) = Rect::from_xywh(*x, *y, *w, *h) {
                p.push_rect(rect);
            }
            let Some(path) = p.finish() else { return Ok(()) };
            s.set_fill(fill.map(|f| Fill { paint: colour(f).into(), ..Fill::default() }));
            s.set_stroke(stroke.then(|| Stroke { paint: colour("#808080").into(), width: 0.5, ..Stroke::default() }));
            s.draw_path(&path);
        }
        Draw::Svg { tree, x, y } => {
            s.push_transform(&Transform::from_translate(*x, *y));
            let size = Size::from_wh(chart::W, chart::H).ok_or_else(|| Error::Pdf("chart size".into()))?;
            s.draw_svg(tree, size, SvgSettings::default()).ok_or_else(|| Error::Pdf("chart".into()))?;
            s.pop();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample;

    #[test]
    fn tagged_pdf() {
        let r = sample::report();
        let bytes = build(&r).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/StructTreeRoot"));
        assert!(text.contains("/MarkInfo"));
        assert!(text.contains("pdfuaid"), "PDF/UA identification in the metadata");
    }

    #[test]
    fn wraps_text() {
        let m = Metrics::new().unwrap();
        let lines = m.wrap("one two three four five six seven", 10.0, false, 60.0);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| m.width(l, 10.0, false) <= 60.0));
        let long = m.wrap(&"x".repeat(200), 10.0, false, 60.0);
        assert!(long.len() > 1 && long.iter().all(|l| m.width(l, 10.0, false) <= 60.0));
        assert_eq!(m.wrap("", 10.0, false, 60.0), vec![String::new()]);
    }
}
