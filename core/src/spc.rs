//! Statistical process control in the NHS "Making Data Count" style: an XmR
//! chart that tells real change from normal ups and downs.
//!
//! Method (published rules; no code taken from any package):
//! - Process limits = mean ± 2.66 × the mean moving range. Moving ranges at
//!   or above 3.267 × their mean are left out before the limits are set.
//! - A point is special cause when it is outside the limits, part of a run
//!   of 7 or more on one side of the mean, part of 7 points each rising (or
//!   each falling), or one of 2 out of 3 points beyond two-thirds of the way
//!   to a limit, all 3 on the same side.
//! - Fewer than `ROBUST_POINTS` points: the limits are shown as "trial".
//! - Assurance compares a fixed target with the limits: outside them means
//!   the target will be met (or missed) every time; inside, it may or may not.

use serde::Serialize;

pub const LIMIT: f64 = 2.66;
pub const SCREEN: f64 = 3.267;
pub const RUN: usize = 7;
pub const ROBUST_POINTS: usize = 15;

/// Which way is better for this measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Better {
    Higher,
    Lower,
    /// Neither: change is shown, but not as good or bad.
    Neither,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Point {
    Common,
    Improvement,
    Concern,
    /// Special cause on a measure with no better direction.
    Neutral,
}

/// The variation icon, from the latest point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Variation {
    pub point: Point,
    /// The special cause is high (above the mean or rising).
    pub high: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Assurance {
    /// The target sits outside the limits on the good side: met every time.
    Pass,
    /// The target sits outside the limits on the bad side: missed every time.
    Fail,
    /// The target sits between the limits: sometimes met, sometimes not.
    Inconsistent,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spc {
    pub mean: f64,
    pub upper: f64,
    pub lower: f64,
    /// One per input value; `None` where the value was blank.
    pub points: Vec<Option<Point>>,
    pub trial: bool,
    pub variation: Variation,
    pub assurance: Option<Assurance>,
}

impl Variation {
    /// Plain words for the icon: "Improvement (higher)".
    pub fn words(&self) -> &'static str {
        match (self.point, self.high) {
            (Point::Common, _) => "No significant change",
            (Point::Improvement, true) => "Improvement (higher)",
            (Point::Improvement, false) => "Improvement (lower)",
            (Point::Concern, true) => "Concern (higher)",
            (Point::Concern, false) => "Concern (lower)",
            (Point::Neutral, true) => "Change (higher)",
            (Point::Neutral, false) => "Change (lower)",
        }
    }
}

impl Assurance {
    pub fn words(&self) -> &'static str {
        match self {
            Assurance::Pass => "Will meet the target every time",
            Assurance::Fail => "Will miss the target every time",
            Assurance::Inconsistent => "May or may not meet the target",
        }
    }
}

/// `None` with fewer than 2 values present: no moving range exists yet.
pub fn analyse(values: &[Option<f64>], better: Better, target: Option<f64>) -> Option<Spc> {
    let present: Vec<(usize, f64)> = values.iter().enumerate().filter_map(|(i, v)| v.map(|v| (i, v))).collect();
    if present.len() < 2 {
        return None;
    }
    let y: Vec<f64> = present.iter().map(|p| p.1).collect();
    let mean = y.iter().sum::<f64>() / y.len() as f64;
    let mut mr: Vec<f64> = y.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
    let amr = mr.iter().sum::<f64>() / mr.len() as f64;
    mr.retain(|&m| m < SCREEN * amr);
    let amr = if mr.is_empty() { amr } else { mr.iter().sum::<f64>() / mr.len() as f64 };
    let (upper, lower) = (mean + LIMIT * amr, mean - LIMIT * amr);
    let (near_up, near_low) = (mean + LIMIT * amr * 2.0 / 3.0, mean - LIMIT * amr * 2.0 / 3.0);

    let n = y.len();
    // +1 = special cause on the high side, -1 low, 0 none. High wins a tie.
    let mut side = vec![0i8; n];
    let mut mark = |i: usize, s: i8| {
        if side[i] == 0 || s > 0 {
            side[i] = s;
        }
    };
    let rel: Vec<i8> = y
        .iter()
        .map(|&v| {
            if v > mean {
                1
            } else if v < mean {
                -1
            } else {
                0
            }
        })
        .collect();
    for (i, &v) in y.iter().enumerate() {
        if v > upper {
            mark(i, 1);
        } else if v < lower {
            mark(i, -1);
        }
    }
    // Runs on one side of the mean.
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j < n && rel[j] == rel[i] && rel[i] != 0 {
            j += 1;
        }
        if j - i >= RUN {
            for k in i..j {
                mark(k, rel[i]);
            }
        }
        i = j.max(i + 1);
    }
    // Trends: RUN points each higher (or lower) than the one before.
    for end in RUN - 1..n {
        let w = &y[end + 1 - RUN..=end];
        let up = w.windows(2).all(|p| p[1] > p[0]);
        let down = w.windows(2).all(|p| p[1] < p[0]);
        if up || down {
            for k in end + 1 - RUN..=end {
                mark(k, if up { 1 } else { -1 });
            }
        }
    }
    // Two of three beyond two-thirds of the limit, all three on one side.
    let beyond: Vec<i8> = y
        .iter()
        .map(|&v| {
            if v > near_up {
                1
            } else if v < near_low {
                -1
            } else {
                0
            }
        })
        .collect();
    for s in 0..n.saturating_sub(2) {
        for dir in [1i8, -1] {
            let same = (s..s + 3).all(|k| rel[k] == dir);
            let close = (s..s + 3).filter(|&k| beyond[k] == dir).count();
            if same && close >= 2 {
                for (k, &b) in beyond.iter().enumerate().skip(s).take(3) {
                    if b == dir {
                        mark(k, dir);
                    }
                }
            }
        }
    }

    let kind = |s: i8| match (s, better) {
        (0, _) => Point::Common,
        (_, Better::Neither) => Point::Neutral,
        (1, Better::Higher) | (-1, Better::Lower) => Point::Improvement,
        _ => Point::Concern,
    };
    let mut points = vec![None; values.len()];
    for (k, &(i, _)) in present.iter().enumerate() {
        points[i] = Some(kind(side[k]));
    }
    let last = side[n - 1];
    let variation = Variation { point: kind(last), high: last > 0 };
    let assurance = match (target, better) {
        (Some(_), Better::Neither) | (None, _) => None,
        (Some(t), b) => Some(if t > upper {
            if b == Better::Higher { Assurance::Fail } else { Assurance::Pass }
        } else if t < lower {
            if b == Better::Higher { Assurance::Pass } else { Assurance::Fail }
        } else {
            Assurance::Inconsistent
        }),
    };
    Some(Spc { mean, upper, lower, points, trial: n < ROBUST_POINTS, variation, assurance })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[f64]) -> Vec<Option<f64>> {
        xs.iter().map(|&x| Some(x)).collect()
    }

    #[test]
    fn limits_by_hand() {
        // Mean 10; moving ranges 2,2,2,1 → mean MR 1.75; limits 10 ± 4.655.
        let s = analyse(&v(&[9.0, 11.0, 9.0, 11.0, 10.0]), Better::Higher, None).unwrap();
        assert!((s.mean - 10.0).abs() < 1e-9);
        assert!((s.upper - 14.655).abs() < 1e-9, "{}", s.upper);
        assert!((s.lower - 5.345).abs() < 1e-9);
        assert!(s.trial);
        assert!(s.points.iter().all(|p| *p == Some(Point::Common)));
        assert_eq!(s.variation.point, Point::Common);
    }

    #[test]
    fn outside_limits_and_screening() {
        let mut xs = vec![10.0, 11.0, 10.0, 11.0, 10.0, 11.0, 10.0, 11.0, 10.0, 11.0, 10.0, 11.0, 10.0, 11.0, 10.0];
        xs.push(40.0);
        let s = analyse(&v(&xs), Better::Lower, None).unwrap();
        // The jump's moving range (29) is screened out, so the limits stay tight.
        assert!(s.upper < 15.0, "{}", s.upper);
        assert_eq!(s.points[15], Some(Point::Concern));
        assert_eq!(s.variation, Variation { point: Point::Concern, high: true });
        assert!(!s.trial);
    }

    #[test]
    fn run_of_seven() {
        let xs = [15.0, 5.0, 15.0, 5.0, 15.0, 5.0, 11.0, 11.0, 11.0, 11.0, 11.0, 11.0, 11.0];
        let s = analyse(&v(&xs), Better::Higher, None).unwrap();
        // Mean ≈ 10.6; the last seven 11s sit above it.
        assert!(s.points[6..].iter().all(|p| *p == Some(Point::Improvement)), "{:?}", s.points);
        assert_eq!(s.points[5], Some(Point::Common));
    }

    #[test]
    fn six_is_not_a_run() {
        let xs = [5.0, 15.0, 5.0, 15.0, 5.0, 15.0, 5.0, 11.0, 11.0, 11.0, 11.0, 11.0, 11.0];
        let s = analyse(&v(&xs), Better::Higher, None).unwrap();
        assert!(s.points[7..].iter().all(|p| *p == Some(Point::Common)), "{:?}", s.points);
    }

    #[test]
    fn trend_of_seven() {
        let xs = [20.0, 30.0, 20.0, 30.0, 20.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0];
        let s = analyse(&v(&xs), Better::Lower, None).unwrap();
        assert!(s.points[4..].iter().all(|p| *p == Some(Point::Concern)), "{:?}", s.points);
        assert!(s.variation.high);
    }

    #[test]
    fn two_of_three_near_the_limit() {
        // Mean 10.743, mean MR 2 → near-limit 14.29, limit 16.06. 14.5 and 14.4
        // pass the near-limit; with 11.5 all three sit above the mean.
        let xs = [9.0, 11.0, 9.0, 11.0, 9.0, 11.0, 9.0, 11.0, 9.0, 11.0, 11.5, 14.5, 14.4, 10.0];
        let s = analyse(&v(&xs), Better::Higher, None).unwrap();
        assert_eq!(s.points[11], Some(Point::Improvement), "{s:?}");
        assert_eq!(s.points[12], Some(Point::Improvement));
        assert_eq!(s.points[10], Some(Point::Common));
    }

    #[test]
    fn assurance() {
        let xs = v(&[9.0, 11.0, 9.0, 11.0, 10.0]);
        let a = |t, b| analyse(&xs, b, Some(t)).unwrap().assurance;
        assert_eq!(a(20.0, Better::Higher), Some(Assurance::Fail));
        assert_eq!(a(20.0, Better::Lower), Some(Assurance::Pass));
        assert_eq!(a(2.0, Better::Higher), Some(Assurance::Pass));
        assert_eq!(a(10.0, Better::Higher), Some(Assurance::Inconsistent));
        assert_eq!(a(10.0, Better::Neither), None);
    }

    #[test]
    fn blanks_are_skipped() {
        let s = analyse(&[Some(1.0), None, Some(3.0)], Better::Higher, None).unwrap();
        assert_eq!(s.points[1], None);
        assert!(analyse(&[Some(1.0), None], Better::Higher, None).is_none());
    }
}
