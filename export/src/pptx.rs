//! pptx: a 16:9 deck with a title slide, the summary table, one
//! chart per slide (alt text on the picture, and the same words shown
//! as a caption so the audience gets the numbers too), and a closing
//! slide with the footnote. Every slide has a real title placeholder.

use std::fmt::Write;

use sachiva_core::report::Report;
use sachiva_core::spc::Assurance;

use crate::chart::{self, H, W};
use crate::ooxml::{EMU_PER_INCH, Package, core_props, esc, rgb};
use crate::{Error, SUMMARY_COLUMNS, fonts, summary_rows};

const SLIDE_W: i64 = 12_192_000;
const SLIDE_H: i64 = 6_858_000;
const SIDE: i64 = EMU_PER_INCH / 2;
const SUMMARY_ROWS_PER_SLIDE: usize = 8;

fn inch(x: f64) -> i64 {
    (x * EMU_PER_INCH as f64) as i64
}

struct Slide {
    shapes: String,
    next_id: u32,
    image: Option<Vec<u8>>,
}

impl Slide {
    fn new(title: &str) -> Slide {
        let mut s = Slide { shapes: String::new(), next_id: 2, image: None };
        let id = s.id();
        let _ = write!(
            s.shapes,
            r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="Title"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{SIDE}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr wrap="square" anchor="b"><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:r><a:rPr lang="en-GB" sz="2800" b="1"><a:solidFill><a:srgbClr val="1A1A1A"/></a:solidFill><a:latin typeface="{f}"/></a:rPr><a:t>{}</a:t></a:r></a:p></p:txBody></p:sp>"#,
            inch(0.25),
            SLIDE_W - 2 * SIDE,
            inch(0.85),
            esc(title),
            f = fonts::OFFICE
        );
        s
    }

    fn id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// A text box across the slide's width.
    fn text(&mut self, y: i64, h: i64, size: u32, italic: bool, lines: &[&str]) {
        let (x, w) = (SIDE, SLIDE_W - 2 * SIDE);
        let id = self.id();
        let i = if italic { r#" i="1""# } else { "" };
        let mut paras = String::new();
        for l in lines {
            let _ = write!(
                paras,
                r#"<a:p><a:r><a:rPr lang="en-GB" sz="{size}"{i}><a:solidFill><a:srgbClr val="1A1A1A"/></a:solidFill><a:latin typeface="{}"/></a:rPr><a:t>{}</a:t></a:r></a:p>"#,
                fonts::OFFICE,
                esc(l)
            );
        }
        let _ = write!(
            self.shapes,
            r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="Text {id}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="square"><a:normAutofit/></a:bodyPr><a:lstStyle/>{paras}</p:txBody></p:sp>"#
        );
    }

    fn picture(&mut self, png: Vec<u8>, alt: &str, x: i64, y: i64, w: i64, h: i64) {
        let id = self.id();
        let _ = write!(
            self.shapes,
            r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="Chart" descr="{}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rIdImage"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#,
            esc(alt)
        );
        self.image = Some(png);
    }

    /// `fills[r][c]`: background and text colour for a body cell.
    fn table(
        &mut self,
        y: i64,
        widths: &[f64],
        head: &[&str],
        rows: &[Vec<String>],
        fills: &[Vec<Option<(&str, &str)>>],
    ) {
        let id = self.id();
        let row_h = inch(0.45);
        let total: i64 = widths.iter().map(|w| inch(*w)).sum();
        let mut grid = String::new();
        for w in widths {
            let _ = write!(grid, r#"<a:gridCol w="{}"/>"#, inch(*w));
        }
        let cell = |text: &str, bold: bool, fill: Option<(&str, &str)>| {
            let (bg, fg) = fill.unwrap_or(if bold { ("#d9d9d9", "#000000") } else { ("#ffffff", "#000000") });
            let b = if bold { r#" b="1""# } else { "" };
            let line = |side: &str| {
                format!(r#"<a:{side} w="6350"><a:solidFill><a:srgbClr val="808080"/></a:solidFill></a:{side}>"#)
            };
            format!(
                r#"<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-GB" sz="1200"{b}><a:solidFill><a:srgbClr val="{}"/></a:solidFill><a:latin typeface="{}"/></a:rPr><a:t>{}</a:t></a:r></a:p></a:txBody><a:tcPr>{}{}{}{}<a:solidFill><a:srgbClr val="{}"/></a:solidFill></a:tcPr></a:tc>"#,
                rgb(fg),
                fonts::OFFICE,
                esc(text),
                line("lnL"),
                line("lnR"),
                line("lnT"),
                line("lnB"),
                rgb(bg)
            )
        };
        let mut body = format!(r#"<a:tr h="{row_h}">"#);
        for h in head {
            body.push_str(&cell(h, true, None));
        }
        body.push_str("</a:tr>");
        for (i, r) in rows.iter().enumerate() {
            let _ = write!(body, r#"<a:tr h="{row_h}">"#);
            for (j, c) in r.iter().enumerate() {
                body.push_str(&cell(c, false, fills.get(i).and_then(|f| f.get(j).copied().flatten())));
            }
            body.push_str("</a:tr>");
        }
        let h = row_h * (rows.len() as i64 + 1);
        let _ = write!(
            self.shapes,
            r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="Table {id}"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="{SIDE}" y="{y}"/><a:ext cx="{total}" cy="{h}"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="1"/><a:tblGrid>{grid}</a:tblGrid>{body}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#
        );
    }

    fn xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
            self.shapes
        )
    }
}

const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;

fn shade(bg: &'static str) -> (&'static str, &'static str) {
    (bg, if bg == chart::NEUTRAL { "#ffffff" } else { "#000000" })
}

pub fn build(r: &Report) -> Result<Vec<u8>, Error> {
    let mut slides = Vec::new();

    let mut first = Slide::new(&r.title);
    first.text(inch(1.3), inch(1.2), 1800, false, &[&r.period, &format!("Made {}", r.made)]);
    slides.push(first);

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
    let pages = rows.len().div_ceil(SUMMARY_ROWS_PER_SLIDE);
    for (p, (chunk, fill)) in rows.chunks(SUMMARY_ROWS_PER_SLIDE).zip(fills.chunks(SUMMARY_ROWS_PER_SLIDE)).enumerate()
    {
        let title = if pages > 1 { format!("Summary ({} of {pages})", p + 1) } else { "Summary".into() };
        let mut s = Slide::new(&title);
        s.table(inch(1.25), &[3.6, 1.0, 1.0, 2.2, 2.6, 1.93], &SUMMARY_COLUMNS, chunk, fill);
        slides.push(s);
    }

    let pic_w = inch(10.0);
    let pic_h = (pic_w as f64 * f64::from(H / W)) as i64;
    for sec in &r.sections {
        for c in &sec.charts {
            let mut s = Slide::new(&format!("{}: {}", sec.title, c.title));
            s.picture(chart::png(c, 2.0)?, &c.alt, (SLIDE_W - pic_w) / 2, inch(1.15), pic_w, pic_h);
            s.text(inch(1.15) + pic_h + inch(0.05), inch(0.95), 1200, false, &[&c.alt]);
            slides.push(s);
        }
    }

    let mut last = Slide::new("About these figures");
    last.text(inch(1.3), inch(2.0), 1600, true, &[&r.footnote]);
    slides.push(last);

    package(r, &slides).map_err(Error::Pptx)
}

fn package(r: &Report, slides: &[Slide]) -> Result<Vec<u8>, String> {
    let mut p = Package::new();
    let mut types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>"#,
    );
    let mut ids = String::new();
    let mut rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdMaster" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/><Relationship Id="rIdTheme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>"#,
    );
    for (i, s) in slides.iter().enumerate() {
        let n = i + 1;
        let _ = write!(
            types,
            r#"<Override PartName="/ppt/slides/slide{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#
        );
        let _ = write!(ids, r#"<p:sldId id="{}" r:id="rIdSlide{n}"/>"#, 255 + n);
        let _ = write!(
            rels,
            r#"<Relationship Id="rIdSlide{n}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{n}.xml"/>"#
        );
        let mut srels = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>"#,
        );
        if let Some(png) = &s.image {
            p.add(&format!("ppt/media/chart{n}.png"), png)?;
            let _ = write!(
                srels,
                r#"<Relationship Id="rIdImage" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/chart{n}.png"/>"#
            );
        }
        srels.push_str("</Relationships>");
        p.add(&format!("ppt/slides/slide{n}.xml"), s.xml().as_bytes())?;
        p.add(&format!("ppt/slides/_rels/slide{n}.xml.rels"), srels.as_bytes())?;
    }
    types.push_str("</Types>");
    rels.push_str("</Relationships>");

    p.add("[Content_Types].xml", types.as_bytes())?;
    p.add("_rels/.rels", ROOT_RELS.as_bytes())?;
    p.add("docProps/core.xml", core_props(&r.title).as_bytes())?;
    p.add(
        "ppt/presentation.xml",
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation {NS}><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rIdMaster"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx="{SLIDE_W}" cy="{SLIDE_H}"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#
        )
        .as_bytes(),
    )?;
    p.add("ppt/_rels/presentation.xml.rels", rels.as_bytes())?;
    p.add("ppt/slideMasters/slideMaster1.xml", master().as_bytes())?;
    p.add("ppt/slideMasters/_rels/slideMaster1.xml.rels", MASTER_RELS.as_bytes())?;
    p.add("ppt/slideLayouts/slideLayout1.xml", layout().as_bytes())?;
    p.add("ppt/slideLayouts/_rels/slideLayout1.xml.rels", LAYOUT_RELS.as_bytes())?;
    p.add("ppt/theme/theme1.xml", theme().as_bytes())?;
    p.finish()
}

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#;

const MASTER_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#;

const LAYOUT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#;

const EMPTY_TREE: &str = r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>"#;

fn title_placeholder() -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{SIDE}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="en-GB"/></a:p></p:txBody></p:sp>"#,
        inch(0.25),
        SLIDE_W - 2 * SIDE,
        inch(0.85)
    )
}

fn master() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldMaster {NS}><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree>{EMPTY_TREE}{}</p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"#,
        title_placeholder()
    )
}

fn layout() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldLayout {NS} type="titleOnly" preserve="1"><p:cSld name="Title only"><p:spTree>{EMPTY_TREE}{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
        title_placeholder()
    )
}

fn theme() -> String {
    let f = fonts::OFFICE;
    let fill = r#"<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>"#;
    let line = format!(r#"<a:ln w="9525">{fill}</a:ln>"#);
    let effect = "<a:effectStyle><a:effectLst/></a:effectStyle>";
    let accents: String = chart::BARS[..6]
        .iter()
        .enumerate()
        .map(|(i, c)| format!(r#"<a:accent{n}><a:srgbClr val="{}"/></a:accent{n}>"#, rgb(c), n = i + 1))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Sachiva"><a:themeElements><a:clrScheme name="Sachiva"><a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1A1A1A"/></a:dk2><a:lt2><a:srgbClr val="F2F2F2"/></a:lt2>{accents}<a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="Sachiva"><a:majorFont><a:latin typeface="{f}"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="{f}"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Sachiva"><a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst><a:lnStyleLst>{line}{line}{line}</a:lnStyleLst><a:effectStyleLst>{effect}{effect}{effect}</a:effectStyleLst><a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#
    )
}

#[cfg(test)]
mod tests {
    use crate::ooxml::check;
    use crate::sample;

    #[test]
    fn deck() {
        let r = sample::report();
        let bytes = super::build(&r).unwrap();
        let parts = check::parts(&bytes);
        let charts: usize = r.sections.iter().map(|s| s.charts.len()).sum();
        let slides = parts.iter().filter(|p| p.0.starts_with("ppt/slides/slide")).count();
        assert_eq!(slides, 1 + r.summary.len().div_ceil(8) + charts + 1);
        let pres = check::text(&parts, "ppt/presentation.xml");
        assert_eq!(pres.matches("<p:sldId ").count(), slides);
        // Every slide has a title placeholder.
        for p in parts.iter().filter(|p| p.0.starts_with("ppt/slides/slide")) {
            assert!(std::str::from_utf8(&p.1).unwrap().contains(r#"<p:ph type="title"/>"#), "{}", p.0);
        }
        let s3 = check::text(&parts, &format!("ppt/slides/slide{}.xml", 2 + r.summary.len().div_ceil(8)));
        assert!(s3.contains(r#"descr=""#) && s3.contains("<p:pic>"));
        let types = check::text(&parts, "[Content_Types].xml");
        assert_eq!(types.matches("presentationml.slide+xml").count(), slides);
        assert_eq!(bytes, super::build(&r).unwrap());
    }
}
