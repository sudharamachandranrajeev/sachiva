//! Draws report charts as SVG: one drawing for every format. PDF embeds it
//! as vector graphics; docx and pptx get a PNG made from it.
//!
//! Run charts use the Making Data Count colours: grey for no significant
//! change, blue for improvement, orange for concern, purple for change on
//! a measure with no better direction. Bars use the Okabe-Ito palette,
//! with a dark outline so even the light colours stand out from white.

use std::fmt::Write;
use std::sync::Arc;

use resvg::{tiny_skia, usvg};
use sachiva_core::report::{Chart, ChartBody, Series};
use sachiva_core::spc::{Point, ROBUST_POINTS, Spc};

use crate::{Error, fonts};

/// Drawing size in points (1/72 inch): 10 x 5 inches.
pub const W: f32 = 720.0;
pub const H: f32 = 360.0;

pub const COMMON: &str = "#a6a6a6";
pub const IMPROVEMENT: &str = "#00b0f0";
pub const CONCERN: &str = "#e46c0a";
pub const NEUTRAL: &str = "#490092";
pub const TARGET: &str = "#de1b1b";
pub const MEAN: &str = "#000000";
pub const LIMITS: &str = "#7f7f7f";
pub const BARS: [&str; 8] = ["#0072b2", "#e69f00", "#009e73", "#cc79a7", "#56b4e9", "#d55e00", "#f0e442", "#000000"];
const INK: &str = "#1a1a1a";
const SOFT: &str = "#595959";
const GRID: &str = "#e0e0e0";
const FONT: &str = "Liberation Sans";

pub fn point_colour(p: Point) -> &'static str {
    match p {
        Point::Common => COMMON,
        Point::Improvement => IMPROVEMENT,
        Point::Concern => CONCERN,
        Point::Neutral => NEUTRAL,
    }
}

enum Key {
    Line(&'static str, bool),
    Dot(&'static str),
    Box(&'static str),
}

struct Frame {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    ymin: f64,
    ymax: f64,
    n: usize,
}

impl Frame {
    fn x(&self, i: usize) -> f32 {
        self.left + (i as f32 + 0.5) * self.slot()
    }
    fn slot(&self) -> f32 {
        (self.right - self.left) / self.n.max(1) as f32
    }
    fn y(&self, v: f64) -> f32 {
        let t = (v - self.ymin) / (self.ymax - self.ymin);
        self.bottom - (t as f32) * (self.bottom - self.top)
    }
}

/// Axis top and tick step: 1, 2 or 5 times a power of ten, about 5 ticks.
fn nice(max: f64) -> (f64, f64) {
    let max = if max.is_finite() && max > 0.0 { max } else { 1.0 };
    let raw = max / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag);
    let step = step.max(1.0);
    ((max / step).ceil() * step, step)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

/// Rough text width, for laying out labels and the legend.
fn width(s: &str, size: f32) -> f32 {
    s.chars().count() as f32 * size * 0.55
}

pub fn svg(chart: &Chart) -> String {
    let n = chart.labels.len();
    let (data_max, data_min) = match &chart.body {
        ChartBody::Spc { values, spc, target, .. } => {
            let mut all: Vec<f64> = values.iter().flatten().copied().collect();
            all.extend(target);
            if let Some(s) = spc {
                all.extend([s.mean, s.upper]);
            }
            (all.iter().copied().fold(0.0, f64::max), all.iter().copied().fold(0.0, f64::min))
        }
        ChartBody::Bars { series, stacked } => {
            let per: Vec<f64> = (0..n)
                .map(|i| {
                    let vals = series.iter().filter_map(|s| s.values.get(i).copied().flatten());
                    if *stacked { vals.sum() } else { vals.fold(0.0, f64::max) }
                })
                .collect();
            (per.iter().copied().fold(0.0, f64::max), 0.0)
        }
    };
    let (ymax, step) = nice(data_max);
    let (ymin, _) = if data_min < 0.0 { nice(-data_min) } else { (0.0, 0.0) };
    let f = Frame {
        left: 64.0,
        right: W - 16.0,
        top: 24.0,
        bottom: H - 72.0,
        ymin: if ymin > 0.0 { -ymin } else { 0.0 },
        ymax,
        n,
    };

    let mut s = String::new();
    let _ = write!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" font-family="{FONT}" font-size="11"><rect width="{W}" height="{H}" fill="#ffffff"/>"##
    );
    // Grid and y axis.
    let mut v = f.ymin;
    while v <= f.ymax + step / 2.0 {
        let y = f.y(v);
        let _ = write!(
            s,
            r#"<line x1="{}" y1="{y}" x2="{}" y2="{y}" stroke="{GRID}" stroke-width="1"/><text x="{}" y="{}" text-anchor="end" fill="{INK}">{}</text>"#,
            f.left,
            f.right,
            f.left - 6.0,
            y + 4.0,
            num(v)
        );
        v += step;
    }
    let mid = (f.top + f.bottom) / 2.0;
    let _ = write!(
        s,
        r#"<text x="14" y="{mid}" transform="rotate(-90 14 {mid})" text-anchor="middle" fill="{INK}">{}</text>"#,
        esc(&chart.y_label)
    );
    let _ = write!(
        s,
        r#"<line x1="{0}" y1="{1}" x2="{2}" y2="{1}" stroke="{INK}" stroke-width="1"/>"#,
        f.left,
        f.y(0.0),
        f.right
    );
    // X labels, thinned so they never overlap.
    let longest = chart.labels.iter().map(|l| width(l, 11.0)).fold(0.0, f32::max) + 8.0;
    let every = ((longest / f.slot()).ceil() as usize).max(1);
    for (i, l) in chart.labels.iter().enumerate() {
        if i % every == 0 {
            let _ = write!(
                s,
                r#"<text x="{}" y="{}" text-anchor="middle" fill="{INK}">{}</text>"#,
                f.x(i),
                f.bottom + 16.0,
                esc(l)
            );
        }
    }

    let keys = match &chart.body {
        ChartBody::Spc { values, spc, target, .. } => run_chart(&mut s, &f, values, spc.as_ref(), *target),
        ChartBody::Bars { series, stacked } => bars(&mut s, &f, series, *stacked),
    };
    let any = match &chart.body {
        ChartBody::Spc { values, .. } => values.iter().any(Option::is_some),
        ChartBody::Bars { series, .. } => series.iter().any(|x| x.values.iter().any(Option::is_some)),
    };
    if !any {
        let _ = write!(
            s,
            r#"<text x="{}" y="{mid}" text-anchor="middle" fill="{SOFT}" font-size="13">No figures entered</text>"#,
            (f.left + f.right) / 2.0
        );
    }
    legend(&mut s, &keys, H - 36.0);
    s.push_str("</svg>");
    s
}

fn run_chart(
    s: &mut String,
    f: &Frame,
    values: &[Option<f64>],
    spc: Option<&Spc>,
    target: Option<f64>,
) -> Vec<(String, Key)> {
    let mut keys = Vec::new();
    let hline = |s: &mut String, v: f64, colour: &str, dash: bool, w: f32| {
        if v >= f.ymin && v <= f.ymax {
            let y = f.y(v);
            let d = if dash { r#" stroke-dasharray="6 4""# } else { "" };
            let _ = write!(
                s,
                r#"<line x1="{}" y1="{y}" x2="{}" y2="{y}" stroke="{colour}" stroke-width="{w}"{d}/>"#,
                f.left, f.right
            );
        }
    };
    if let Some(spc) = spc {
        hline(s, spc.upper, LIMITS, true, 1.2);
        hline(s, spc.lower, LIMITS, true, 1.2);
        hline(s, spc.mean, MEAN, false, 1.2);
        keys.push(("Average".to_string(), Key::Line(MEAN, false)));
        keys.push((
            if spc.trial { "Process limits (trial)" } else { "Process limits" }.to_string(),
            Key::Line(LIMITS, true),
        ));
        if spc.trial {
            let _ = write!(
                s,
                r#"<text x="{}" y="14" text-anchor="end" fill="{SOFT}" font-style="italic">Trial limits: fewer than {ROBUST_POINTS} points</text>"#,
                f.right
            );
        }
    }
    if let Some(t) = target {
        hline(s, t, TARGET, true, 1.6);
        keys.push(("Target".to_string(), Key::Line(TARGET, true)));
    }
    // The line breaks where a value is blank.
    let mut d = String::new();
    let mut pen = false;
    for (i, v) in values.iter().enumerate() {
        match v {
            Some(v) => {
                let _ = write!(d, "{}{} {} ", if pen { "L" } else { "M" }, f.x(i), f.y(*v));
                pen = true;
            }
            None => pen = false,
        }
    }
    if !d.is_empty() {
        let _ = write!(s, r#"<path d="{}" fill="none" stroke="{COMMON}" stroke-width="1.8"/>"#, d.trim_end());
    }
    let mut seen = Vec::new();
    for (i, v) in values.iter().enumerate() {
        if let Some(v) = v {
            let p = spc.and_then(|x| x.points.get(i).copied().flatten()).unwrap_or(Point::Common);
            if !seen.contains(&p) {
                seen.push(p);
            }
            let _ = write!(s, r#"<circle cx="{}" cy="{}" r="4" fill="{}"/>"#, f.x(i), f.y(*v), point_colour(p));
        }
    }
    for p in [Point::Common, Point::Improvement, Point::Concern, Point::Neutral] {
        if seen.contains(&p) {
            let name = match p {
                Point::Common => "No significant change",
                Point::Improvement => "Improvement",
                Point::Concern => "Concern",
                Point::Neutral => "Change",
            };
            keys.push((name.to_string(), Key::Dot(point_colour(p))));
        }
    }
    keys
}

fn bars(s: &mut String, f: &Frame, series: &[Series], stacked: bool) -> Vec<(String, Key)> {
    let k = series.len().max(1) as f32;
    let group = f.slot() * 0.75;
    let each = if stacked { group } else { group / k };
    for i in 0..f.n {
        let mut base = 0.0;
        for (j, sr) in series.iter().enumerate() {
            let Some(v) = sr.values.get(i).copied().flatten() else { continue };
            let colour = BARS[j % BARS.len()];
            let (x, y0, y1) = if stacked {
                let r = (f.x(i) - group / 2.0, f.y(base), f.y(base + v));
                base += v;
                r
            } else {
                (f.x(i) - group / 2.0 + j as f32 * each, f.y(0.0), f.y(v))
            };
            let _ = write!(
                s,
                r##"<rect x="{x}" y="{}" width="{each}" height="{}" fill="{colour}" stroke="#333333" stroke-width="0.5"/>"##,
                y1.min(y0),
                (y0 - y1).abs()
            );
        }
    }
    series.iter().enumerate().map(|(j, sr)| (sr.name.clone(), Key::Box(BARS[j % BARS.len()]))).collect()
}

fn legend(s: &mut String, keys: &[(String, Key)], mut y: f32) {
    let mut x = 64.0;
    for (name, key) in keys {
        let w = 24.0 + width(name, 11.0) + 18.0;
        if x + w > W - 16.0 {
            x = 64.0;
            y += 16.0;
        }
        match key {
            Key::Line(c, dash) => {
                let d = if *dash { r#" stroke-dasharray="6 4""# } else { "" };
                let _ = write!(
                    s,
                    r#"<line x1="{x}" y1="{y}" x2="{}" y2="{y}" stroke="{c}" stroke-width="1.6"{d}/>"#,
                    x + 18.0
                );
            }
            Key::Dot(c) => {
                let _ = write!(s, r#"<circle cx="{}" cy="{y}" r="4" fill="{c}"/>"#, x + 9.0);
            }
            Key::Box(c) => {
                let _ = write!(
                    s,
                    r##"<rect x="{}" y="{}" width="12" height="10" fill="{c}" stroke="#333333" stroke-width="0.5"/>"##,
                    x + 3.0,
                    y - 5.0
                );
            }
        }
        let _ = write!(s, r#"<text x="{}" y="{}" fill="{INK}">{}</text>"#, x + 24.0, y + 4.0, esc(name));
        x += w;
    }
}

pub fn tree(svg: &str) -> Result<usvg::Tree, Error> {
    let opt = usvg::Options { fontdb: Arc::new(fonts::database()), font_family: FONT.into(), ..Default::default() };
    usvg::Tree::from_str(svg, &opt).map_err(|e| Error::Chart(e.to_string()))
}

/// The chart as a PNG at `scale` pixels per point.
pub fn png(chart: &Chart, scale: f32) -> Result<Vec<u8>, Error> {
    let t = tree(&svg(chart))?;
    let mut pixmap = tiny_skia::Pixmap::new((W * scale) as u32, (H * scale) as u32)
        .ok_or_else(|| Error::Chart("empty image".into()))?;
    resvg::render(&t, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| Error::Chart(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sachiva_core::spc::{Better, analyse};

    fn spc_chart(values: Vec<Option<f64>>) -> Chart {
        let spc = analyse(&values, Better::Higher, Some(10.0));
        Chart {
            title: "Calls per week".into(),
            alt: "alt".into(),
            y_label: "Calls <&>".into(),
            labels: (0..values.len()).map(|i| format!("w/c {i} Nov 2026")).collect(),
            body: ChartBody::Spc { values, spc, target: Some(10.0), better: Better::Higher },
        }
    }

    #[test]
    fn nice_axis() {
        assert_eq!(nice(47.0), (50.0, 10.0));
        assert_eq!(nice(3.0), (3.0, 1.0));
        assert_eq!(nice(0.0), (1.0, 1.0));
        assert_eq!(nice(1234.0), (1500.0, 500.0));
    }

    #[test]
    fn gaps_break_the_line_and_text_is_escaped() {
        let c = spc_chart(vec![Some(8.0), Some(12.0), None, Some(9.0), Some(11.0)]);
        let s = svg(&c);
        assert!(s.contains("Calls &lt;&amp;&gt;"));
        let path = s.split(r#"<path d=""#).nth(1).unwrap().split('"').next().unwrap();
        assert_eq!(path.matches('M').count(), 2, "{path}");
        assert_eq!(s.matches("<circle").count(), 4 + 1, "4 points and one legend dot");
        assert!(s.contains("Trial limits"));
        assert!(s.contains(TARGET));
    }

    #[test]
    fn renders_png_and_handles_empty() {
        let png = png(&spc_chart(vec![Some(8.0), Some(12.0), Some(9.0)]), 1.0).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let empty = spc_chart(vec![None, None]);
        assert!(svg(&empty).contains("No figures entered"));
        let bars = Chart {
            body: ChartBody::Bars {
                series: vec![
                    Series { name: "Added".into(), values: vec![Some(3.0), None] },
                    Series { name: "Removed".into(), values: vec![Some(2.0), Some(1.0)] },
                ],
                stacked: true,
            },
            ..empty
        };
        let s = svg(&bars);
        assert_eq!(s.matches("<rect").count(), 1 + 3 + 2, "background, 3 bars, 2 legend boxes");
        tree(&s).unwrap();
    }
}
