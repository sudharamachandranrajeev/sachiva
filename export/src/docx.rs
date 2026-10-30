//! docx: an A4 landscape report with real headings, the summary table,
//! each chart as a picture with alt text, each data table with a header
//! row that repeats across pages, and the footnote.

use std::fmt::Write;

use sachiva_core::report::Report;
use sachiva_core::spc::Assurance;

use crate::chart::{self, H, W};
use crate::ooxml::{EMU_PER_INCH, Package, core_props, esc, rgb};
use crate::{Error, SUMMARY_COLUMNS, cell_text, fonts, summary_rows};

/// A4 landscape with 0.75 inch margins, in twentieths of a point.
const PAGE_W: u32 = 16838;
const PAGE_H: u32 = 11906;
const MARGIN: u32 = 1080;
const TEXT_W: u32 = PAGE_W - 2 * MARGIN;
/// Charts are drawn 10 x 5 inches; on the page they are 9.5 inches wide.
const PIC_W: i64 = EMU_PER_INCH * 95 / 10;
const PIC_H: i64 = (PIC_W as f64 * (H / W) as f64) as i64;

struct Doc {
    body: String,
    images: Vec<Vec<u8>>,
}

impl Doc {
    /// `Heading2Break` is a level-2 heading that starts a new page.
    fn para(&mut self, style: &str, text: &str) {
        let st = match style {
            "" => String::new(),
            "Heading1Break" => r#"<w:pPr><w:pStyle w:val="Heading1"/><w:pageBreakBefore/></w:pPr>"#.into(),
            "Heading2Break" => r#"<w:pPr><w:pStyle w:val="Heading2"/><w:pageBreakBefore/></w:pPr>"#.into(),
            s => format!(r#"<w:pPr><w:pStyle w:val="{s}"/></w:pPr>"#),
        };
        let _ = write!(self.body, r#"<w:p>{st}<w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#, esc(text));
    }

    /// `fills[i]` shades cell `i` of a row: (background, text colour).
    fn table(&mut self, columns: &[String], rows: &[Vec<String>], fills: &[Vec<Option<(&str, &str)>>]) {
        let n = columns.len().max(1) as u32;
        let col = TEXT_W / n;
        let _ = write!(
            self.body,
            r#"<w:tbl><w:tblPr><w:tblStyle w:val="Grid"/><w:tblW w:w="{TEXT_W}" w:type="dxa"/><w:tblLook w:val="0420" w:firstRow="1" w:lastRow="0" w:firstColumn="0" w:lastColumn="0" w:noHBand="1" w:noVBand="1"/></w:tblPr><w:tblGrid>"#
        );
        for _ in 0..n {
            let _ = write!(self.body, r#"<w:gridCol w:w="{col}"/>"#);
        }
        self.body.push_str("</w:tblGrid>");
        let cell = |b: &mut String, text: &str, bold: bool, fill: Option<(&str, &str)>| {
            let shade = match fill {
                Some((bg, _)) => format!(r#"<w:shd w:val="clear" w:color="auto" w:fill="{}"/>"#, rgb(bg)),
                None if bold => r#"<w:shd w:val="clear" w:color="auto" w:fill="D9D9D9"/>"#.into(),
                None => String::new(),
            };
            let mut rpr = String::new();
            if bold {
                rpr.push_str("<w:b/>");
            }
            if let Some((_, fg)) = fill {
                let _ = write!(rpr, r#"<w:color w:val="{}"/>"#, rgb(fg));
            }
            let _ = write!(
                b,
                r#"<w:tc><w:tcPr><w:tcW w:w="{col}" w:type="dxa"/>{shade}</w:tcPr><w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr><w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
                esc(text)
            );
        };
        self.body.push_str(r#"<w:tr><w:trPr><w:tblHeader/><w:cantSplit/></w:trPr>"#);
        for c in columns {
            cell(&mut self.body, c, true, None);
        }
        self.body.push_str("</w:tr>");
        for (i, r) in rows.iter().enumerate() {
            self.body.push_str(r#"<w:tr><w:trPr><w:cantSplit/></w:trPr>"#);
            for (j, c) in r.iter().enumerate() {
                let f = fills.get(i).and_then(|f| f.get(j).copied().flatten());
                cell(&mut self.body, c, false, f);
            }
            self.body.push_str("</w:tr>");
        }
        self.body.push_str("</w:tbl>");
        self.para("", "");
    }

    fn picture(&mut self, png: Vec<u8>, alt: &str) {
        self.images.push(png);
        let n = self.images.len();
        let alt = esc(alt);
        let _ = write!(
            self.body,
            r#"<w:p><w:pPr><w:keepNext/></w:pPr><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{PIC_W}" cy="{PIC_H}"/><wp:docPr id="{n}" name="Chart {n}" descr="{alt}"/><wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" noChangeAspect="1"/></wp:cNvGraphicFramePr><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="{n}" name="chart{n}.png" descr="{alt}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdImage{n}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{PIC_W}" cy="{PIC_H}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
        );
    }
}

pub fn build(r: &Report) -> Result<Vec<u8>, Error> {
    let mut d = Doc { body: String::new(), images: Vec::new() };
    d.para("Title", &r.title);
    d.para("", &r.period);
    d.para("", &format!("Made {}", r.made));

    d.para("Heading1", "Summary");
    let cols: Vec<String> = SUMMARY_COLUMNS.iter().map(|s| s.to_string()).collect();
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
        d.para("", "Nothing to show for this period.");
    } else {
        d.table(&cols, &rows, &fills);
    }

    for s in &r.sections {
        d.para("Heading1Break", &s.title);
        if !s.intro.is_empty() {
            d.para("", &s.intro);
        }
        // Two charts never fit one page: each further chart starts a new one.
        for (i, c) in s.charts.iter().enumerate() {
            d.para(if i == 0 { "Heading2" } else { "Heading2Break" }, &c.title);
            d.picture(chart::png(c, 2.0)?, &c.alt);
        }
        d.para("Heading2", "Figures");
        let rows: Vec<Vec<String>> = s.table.rows.iter().map(|row| row.iter().map(cell_text).collect()).collect();
        d.table(&s.table.columns, &rows, &[]);
    }
    d.para("Footnote", &r.footnote);

    let mut p = Package::new();
    let e = Error::Docx;
    p.add("[Content_Types].xml", CONTENT_TYPES.as_bytes()).map_err(e)?;
    p.add("_rels/.rels", ROOT_RELS.as_bytes()).map_err(e)?;
    p.add("docProps/core.xml", core_props(&r.title).as_bytes()).map_err(e)?;
    p.add("word/styles.xml", styles().as_bytes()).map_err(e)?;
    let mut rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    );
    for (i, png) in d.images.iter().enumerate() {
        let n = i + 1;
        let _ = write!(
            rels,
            r#"<Relationship Id="rIdImage{n}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/chart{n}.png"/>"#
        );
        p.add(&format!("word/media/chart{n}.png"), png).map_err(e)?;
    }
    rels.push_str("</Relationships>");
    p.add("word/_rels/document.xml.rels", rels.as_bytes()).map_err(e)?;
    let doc = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><w:body>{}<w:sectPr><w:pgSz w:w="{PAGE_W}" w:h="{PAGE_H}" w:orient="landscape"/><w:pgMar w:top="{MARGIN}" w:right="{MARGIN}" w:bottom="{MARGIN}" w:left="{MARGIN}" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>"#,
        d.body
    );
    p.add("word/document.xml", doc.as_bytes()).map_err(e)?;
    p.finish().map_err(e)
}

/// Background and a text colour that reads on it.
fn shade(bg: &'static str) -> (&'static str, &'static str) {
    (bg, if bg == chart::NEUTRAL { "#FFFFFF" } else { "#000000" })
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/></Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#;

fn styles() -> String {
    let f = fonts::OFFICE;
    let heading = |id: &str, name: &str, size: u32, level: u32| {
        format!(
            r#"<w:style w:type="paragraph" w:styleId="{id}"><w:name w:val="{name}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="120"/><w:outlineLvl w:val="{level}"/></w:pPr><w:rPr><w:b/><w:sz w:val="{size}"/></w:rPr></w:style>"#
        )
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="{f}" w:hAnsi="{f}" w:cs="{f}" w:eastAsia="{f}"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="en-GB"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="276" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="240"/></w:pPr><w:rPr><w:b/><w:sz w:val="48"/></w:rPr></w:style>{}{}<w:style w:type="paragraph" w:styleId="Footnote"><w:name w:val="Footnote Text"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="240"/></w:pPr><w:rPr><w:i/><w:sz w:val="18"/></w:rPr></w:style><w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:tblPr><w:tblInd w:w="0" w:type="dxa"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style><w:style w:type="table" w:styleId="Grid"><w:name w:val="Table Grid"/><w:basedOn w:val="TableNormal"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:left w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:right w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="808080"/></w:tblBorders></w:tblPr></w:style></w:styles>"#,
        heading("Heading1", "heading 1", 32, 0),
        heading("Heading2", "heading 2", 26, 1)
    )
}

#[cfg(test)]
mod tests {
    use crate::ooxml::check;
    use crate::sample;

    #[test]
    fn word_document() {
        let r = sample::report();
        let bytes = super::build(&r).unwrap();
        let parts = check::parts(&bytes);
        let doc = check::text(&parts, "word/document.xml");
        assert!(doc.contains("Follow-ups &lt;A &amp; B&gt;"));
        assert!(doc.contains(r#"<w:pStyle w:val="Heading1"/>"#));
        assert!(doc.contains("<w:tblHeader/>"));
        let charts: usize = r.sections.iter().map(|s| s.charts.len()).sum();
        assert_eq!(doc.matches("<wp:docPr ").count(), charts);
        assert!(doc.contains(&format!(r#"descr="{}""#, crate::ooxml::esc(&r.sections[0].charts[0].alt))));
        assert_eq!(parts.iter().filter(|p| p.0.ends_with(".png")).count(), charts);
        // Blank figures show as a dash.
        assert!(doc.contains(">–<"));
        // Same report, same bytes.
        assert_eq!(bytes, super::build(&r).unwrap());
    }
}
