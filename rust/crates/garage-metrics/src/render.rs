//! Rendering
//!
//! Every number that leaves this module is a byte in a file Waybar rasterises, so the
//! format specs below are the Python's exactly: `{:.2f}` on a coordinate, `{:.1f}` on the
//! midline, `{:g}` on the icon transform, and a bare integer wherever the Python
//! interpolates one of the layout table's `int`s. See [`crate::pyfmt`] for why those
//! agree between the two languages.

use crate::data::{
    icon_path, layout, Layout, GRAPH_BOTTOM, GRAPH_HEIGHT, GRAPH_TOP, ICON_OPACITY, ICON_SIZE,
    POINTS,
};
use crate::files::{as_int, count_as_float};
use crate::json::{Object, Value};
use crate::pyfmt::{general, html_escape};

/// One stored reading as a number between 0 and 100, or 0 if it is neither.
///
/// `float(value)` in the Python accepts a numeric string as well as a number, which is
/// why a text reading is parsed here rather than rejected; everything else -- `None`, a
/// list, a word -- is a `TypeError` or a `ValueError` and becomes zero. A `NaN` clamps to
/// zero too, because Python's `max(0.0, nan)` keeps the 0.0 it started from.
pub(crate) fn clamp_sample(value: &Value) -> f64 {
    let number = match value {
        Value::Str(text) => text.trim().parse().ok(),
        Value::Null
        | Value::Bool(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::List(_)
        | Value::Object(_) => value.as_number(),
    };
    match number {
        Some(number) if !number.is_nan() => number.clamp(0.0, 100.0),
        _ => 0.0,
    }
}

/// The history reduced to about one point per pixel of graph.
///
/// `POINTS` is 120 and a strip's graph is 22px wide, so a line drawn straight from the
/// history puts five vertices in every pixel column. On a steady series that is
/// invisible; on a volatile one -- a GPU under load moving tens of percent a second --
/// the five disagree, and what renders is a vertical hash rather than a line. There is no
/// more detail in it than the mean, only more ink: the strip does not have the pixels to
/// say what those five samples did, and the dashboard, which does, plots the same history
/// unreduced.
///
/// The mean rather than the maximum of each bucket, because the maximum of five samples
/// is what the busiest instant did and a strip of those reads as a machine permanently at
/// full tilt.
///
/// The bucket sum runs left to right from zero, as Python's `sum()` does, so the two
/// builds accumulate the same rounding error and land on the same double.
pub(crate) fn resample(samples: &[f64], columns: usize) -> Vec<f64> {
    if columns < 2 || samples.len() <= columns {
        return samples.to_vec();
    }
    let mut reduced = Vec::with_capacity(columns);
    for index in 0..columns {
        let start = index * samples.len() / columns;
        let end = ((index + 1) * samples.len() / columns).max(start + 1);
        let bucket = samples.get(start..end).unwrap_or_default();
        let total: f64 = bucket.iter().sum();
        reduced.push(total / count_as_float(bucket.len()));
    }
    reduced
}

/// The stored history as points, plotted straight.
///
/// The axis is linear because every history that needs a logarithm has already had one.
/// Throughput is stored through `log_scale()` at the moment it is sampled, since idle is
/// 0.01 MiB/s and an `NVMe` burst is 3000; the rest are percentages, and a percentage is
/// already the scale it wants to be read on.
///
/// Curving them again was worse than redundant. `log1p` put 1% of load at 15% of the
/// graph's height and 20% at 66%, so a card ticking over near idle drew a line thrashing
/// across the full height of the strip -- and a 45C package reading sat just under the top
/// of its graph.
pub(crate) fn coordinates(history: &[Value], graph_x: f64, graph_width: f64) -> Vec<(f64, f64)> {
    let clamped: Vec<f64> = history.iter().map(clamp_sample).collect();
    // `int(round(graph_width)) + 1`: Python's `round` is half-to-even and Rust's
    // `f64::round` is half-away-from-zero, so this goes through py_round rather than
    // the method. Every width in the layout table is a whole number, which makes the
    // two agree today and would stop making them agree the moment one is not.
    let columns = (as_int(crate::pyfmt::py_round(graph_width, 0)) + 1).max(2);
    let mut samples = resample(&clamped, usize::try_from(columns).unwrap_or(2));
    if samples.len() == 1 {
        samples = vec![samples.first().copied().unwrap_or(0.0); 2];
    }
    let span = count_as_float(samples.len().saturating_sub(1).max(1));
    samples
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = graph_x + count_as_float(index) * graph_width / span;
            let y = f64::from(GRAPH_BOTTOM) - value / 100.0 * f64::from(GRAPH_HEIGHT);
            (x, y)
        })
        .collect()
}

/// One Phosphor glyph in the strip's shared 14px icon box.
fn render_glyph(name: &str, x: f64, foreground: &str) -> String {
    let top = (22.0 - ICON_SIZE) / 2.0;
    format!(
        concat!(
            r#"<g transform="translate({} {}) scale({:.6})" fill="{}" fill-opacity="{}">"#,
            r#"<path d="{}"/></g>"#
        ),
        general(x),
        general(top),
        ICON_SIZE / 256.0,
        foreground,
        ICON_OPACITY,
        icon_path(name)
    )
}

/// The graph: a midline, a baseline, a filled area, the primary line, and the secondary
/// line where there is one.
///
/// The second series gets no fill and a thinner, dimmer stroke: two filled areas at 8%
/// stack into a muddy block where they overlap, and the whole point of the second line is
/// that it reads as subordinate to the first.
fn render_graph(layout: Layout, state: &Object, foreground: &str) -> String {
    let graph_x = layout.graph_x.unwrap_or(0);
    let graph_width = layout.graph_width.unwrap_or(0);
    let flat = vec![Value::Float(0.0); POINTS];
    let history = series(state, "history").unwrap_or(&flat);
    let points = coordinates(history, f64::from(graph_x), f64::from(graph_width));
    let midline = f64::from(GRAPH_TOP) + f64::from(GRAPH_HEIGHT) / 2.0;
    let secondary = series(state, "history2").map_or_else(String::new, |history| {
        let second = coordinates(history, f64::from(graph_x), f64::from(graph_width));
        format!(
            concat!(
                "\n  <polyline points=\"{}\" fill=\"none\" stroke=\"{}\" ",
                r#"stroke-opacity="0.34" stroke-width="1.1" stroke-linecap="round" "#,
                "stroke-linejoin=\"round\"/>"
            ),
            polyline(&second),
            foreground
        )
    });
    let opacity = if state.get("active").is_some_and(Value::is_truthy) {
        "0.92"
    } else {
        "0.62"
    };
    format!(
        concat!(
            "<path d=\"M {graph_x} {midline:.1} H {right}\" stroke=\"{fg}\" ",
            "stroke-opacity=\"0.10\" stroke-width=\"1\"/>\n",
            "  <path d=\"M {graph_x} {bottom} H {right}\" stroke=\"{fg}\" ",
            "stroke-opacity=\"0.16\" stroke-width=\"1\"/>\n",
            "  <path d=\"{area}\" fill=\"{fg}\" fill-opacity=\"0.08\"/>\n",
            "  <polyline points=\"{line}\" fill=\"none\" stroke=\"{fg}\" ",
            "stroke-opacity=\"{opacity}\" stroke-width=\"1.4\" stroke-linecap=\"round\" ",
            "stroke-linejoin=\"round\"/>{secondary}"
        ),
        graph_x = graph_x,
        midline = midline,
        right = graph_x + graph_width,
        bottom = GRAPH_BOTTOM,
        fg = foreground,
        area = area(&points),
        line = polyline(&points),
        opacity = opacity,
        secondary = secondary,
    )
}

/// A history, but only when it is a list with something in it -- the Python's
/// `state.get("history") or [...]` and `isinstance(history2, list) and history2`.
fn series<'a>(state: &'a Object, key: &str) -> Option<&'a [Value]> {
    state
        .get(key)
        .and_then(Value::as_list)
        .filter(|history| !history.is_empty())
}

fn polyline(points: &[(f64, f64)]) -> String {
    points
        .iter()
        .map(|(x, y)| format!("{x:.2},{y:.2}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The filled area under the line: down to the baseline at the left edge, along the line,
/// down to the baseline at the right edge, closed.
///
/// The two baseline coordinates interpolate `GRAPH_BOTTOM` as the bare integer 19 while
/// the line's own y values carry two decimals, which is the Python's asymmetry and not a
/// slip -- one is a constant and the other is a computed height.
fn area(points: &[(f64, f64)]) -> String {
    let first = points.first().map_or(0.0, |(x, _)| *x);
    let last = points.last().map_or(0.0, |(x, _)| *x);
    let body = points
        .iter()
        .map(|(x, y)| format!("L {x:.2} {y:.2}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("M {first:.2} {GRAPH_BOTTOM} {body} L {last:.2} {GRAPH_BOTTOM} Z")
}

/// Optional icon and graph, then the strip's one or two readings.
///
/// Network has neither: its down/up arrows label the figures directly. Keeping icon and
/// graph independently optional avoids reserving an empty leading box.
fn render_widget(widget: &str, state: &Object, foreground: &str) -> String {
    let Some(layout) = layout(widget) else {
        return String::new();
    };
    let display = escaped(state, "display", "--");
    let extra = escaped(state, "extra", "");
    let extra_text = match layout.extra_x {
        Some(extra_x) if !extra.is_empty() => {
            format!("\n  <text x=\"{extra_x}\" y=\"11\" class=\"secondary\">{extra}</text>")
        }
        _ => String::new(),
    };
    let icon = match (layout.direction_icons, layout.icon) {
        (Some((down_x, up_x)), _) => format!(
            "{}\n  {}\n  ",
            render_glyph("arrow-down", f64::from(down_x), foreground),
            render_glyph("arrow-up", f64::from(up_x), foreground)
        ),
        (None, Some(name)) => format!("{}\n  ", render_glyph(name, 0.0, foreground)),
        (None, None) => String::new(),
    };
    let graph = match layout.graph_width {
        Some(_) => format!("{}\n  ", render_graph(layout, state, foreground)),
        None => String::new(),
    };
    let value_x = layout.value_x;
    format!("{icon}{graph}<text x=\"{value_x}\" y=\"11\">{display}</text>{extra_text}")
}

/// `html.escape(str(state.get(key, fallback)))`, which is two conversions and both
/// matter: a key that is present and `null` spells `None` rather than falling back.
fn escaped(state: &Object, key: &str, fallback: &str) -> String {
    let text = state
        .get(key)
        .map_or_else(|| fallback.to_string(), Value::py_str);
    html_escape(&text)
}

/// The whole strip: a 22px-tall SVG whose one style rule is the bar's own face.
pub(crate) fn render_svg(widget: &str, state: &Object, foreground: &str) -> String {
    let width = layout(widget).map_or(0, |layout| layout.width);
    format!(
        concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"22\" ",
            "viewBox=\"0 0 {width} 22\">\n",
            "  <style>\n",
            "    text {{ font-family: \"Plus Jakarta Sans\", sans-serif; font-size: 11.5px; ",
            "font-weight: 600; dominant-baseline: central; fill: {fg}; fill-opacity: 1.0; }}\n",
            "    .secondary {{ fill-opacity: 0.75; }}\n",
            "  </style>\n",
            "  {body}\n",
            "</svg>\n"
        ),
        width = width,
        fg = foreground,
        body = render_widget(widget, state, foreground),
    )
}

#[cfg(test)]
// Byte-parity tests: a fixture row of the wrong shape is a broken fixture and panicking
// on it is the report, and a double that is only approximately the Python's is a failure
// rather than a pass -- so indexing and exact float comparison are both the point here.
#[allow(
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]
mod tests {
    use super::{area, clamp_sample, coordinates, polyline, render_svg, resample};
    use crate::data::POINTS;
    use crate::json::{object, Object, Value};

    /// `<columns>|<space-separated inputs>|<space-separated expected>` for each row,
    /// produced by running the Python's `_resample` over a corpus and checked in.
    const RESAMPLE: &str = include_str!("../testdata/resample.txt");

    /// `<graph_x> <graph_width>|<inputs>|<x,y ...>` from the Python's `_coordinates`,
    /// formatted with the same `{:.2f}` the SVG uses so the fixture pins the bytes and
    /// not just the doubles.
    const COORDINATES: &str = include_str!("../testdata/coordinates.txt");

    fn numbers(field: &str) -> Vec<f64> {
        field
            .split_whitespace()
            .map(|value| value.parse().expect("a float"))
            .collect()
    }

    #[test]
    fn resample_matches_the_python_over_the_whole_fixture() {
        let mut rows = 0;
        for line in RESAMPLE.lines().filter(|line| !line.is_empty()) {
            let fields: Vec<&str> = line.split('|').collect();
            let columns: usize = fields[0].parse().expect("a column count");
            let got = resample(&numbers(fields[1]), columns);
            let want = numbers(fields[2]);
            assert_eq!(got.len(), want.len(), "row {rows}");
            for (got, want) in got.iter().zip(want.iter()) {
                assert!((got - want).abs() < 1e-12, "row {rows}: {got} vs {want}");
            }
            rows += 1;
        }
        assert!(rows >= 20, "fixture shrank to {rows} rows");
    }

    #[test]
    fn coordinates_match_the_python_byte_for_byte() {
        let mut rows = 0;
        for line in COORDINATES.lines().filter(|line| !line.is_empty()) {
            let fields: Vec<&str> = line.split('|').collect();
            let geometry = numbers(fields[0]);
            let history: Vec<Value> = numbers(fields[1]).into_iter().map(Value::Float).collect();
            let points = coordinates(&history, geometry[0], geometry[1]);
            assert_eq!(polyline(&points), fields[2], "row {rows}");
            rows += 1;
        }
        assert!(rows >= 10, "fixture shrank to {rows} rows");
    }

    #[test]
    fn a_short_history_is_returned_unreduced() {
        assert_eq!(resample(&[1.0, 2.0, 3.0], 23), vec![1.0, 2.0, 3.0]);
        assert_eq!(resample(&[1.0, 2.0, 3.0], 1), vec![1.0, 2.0, 3.0]);
        assert_eq!(resample(&[], 23), Vec::<f64>::new());
    }

    #[test]
    fn buckets_are_means_rather_than_maxima() {
        assert_eq!(resample(&[0.0, 100.0, 0.0, 100.0], 2), vec![50.0, 50.0]);
    }

    #[test]
    fn a_reading_that_is_not_a_number_clamps_to_zero() {
        assert_eq!(clamp_sample(&Value::Null), 0.0);
        assert_eq!(clamp_sample(&Value::str("nonsense")), 0.0);
        assert_eq!(clamp_sample(&Value::List(vec![])), 0.0);
        assert_eq!(clamp_sample(&Value::Float(f64::NAN)), 0.0);
    }

    #[test]
    fn a_reading_outside_nought_to_a_hundred_is_pulled_back_in() {
        assert_eq!(clamp_sample(&Value::Float(-5.0)), 0.0);
        assert_eq!(clamp_sample(&Value::Float(140.0)), 100.0);
        assert_eq!(clamp_sample(&Value::Int(0)), 0.0);
        assert_eq!(clamp_sample(&Value::Int(42)), 42.0);
        assert_eq!(clamp_sample(&Value::Bool(true)), 1.0);
        // float("1.5") is a number in Python, so a stringly-typed history still draws.
        assert_eq!(clamp_sample(&Value::str("1.5")), 1.5);
    }

    #[test]
    fn a_single_point_history_is_doubled_so_a_line_has_two_ends() {
        let points = coordinates(&[Value::Float(50.0)], 20.0, 22.0);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0], (20.0, 11.0));
        assert_eq!(points[1], (42.0, 11.0));
    }

    #[test]
    fn the_baseline_in_an_area_path_is_the_bare_integer_the_python_writes() {
        let path = area(&[(20.0, 19.0), (42.0, 3.0)]);
        assert_eq!(path, "M 20.00 19 L 20.00 19.00 L 42.00 3.00 L 42.00 19 Z");
    }

    #[test]
    fn a_full_history_renders_the_same_bytes_as_the_python() {
        const EXPECTED: &str = include_str!("../testdata/svg/cpu-ramp.svg");
        let history: Vec<Value> = (0..POINTS)
            .map(|index| Value::Float((index % 100) as f64))
            .collect();
        let state = object! {
            "history" => Value::List(history),
            "display" => Value::str("42%"),
            "active" => Value::Bool(true),
        };
        assert_eq!(render_svg("cpu", &state, "#f5f5f7"), EXPECTED);
    }

    #[test]
    fn a_state_with_nothing_in_it_still_renders_a_flat_strip() {
        const EXPECTED: &str = include_str!("../testdata/svg/temp-empty.svg");
        assert_eq!(render_svg("temp", &Object::new(), "#f5f5f7"), EXPECTED);
    }

    #[test]
    fn the_network_strip_carries_two_arrows_and_no_graph() {
        const EXPECTED: &str = include_str!("../testdata/svg/network-idle.svg");
        let state = object! {
            "display" => Value::str("0K"),
            "extra" => Value::str("0K"),
        };
        assert_eq!(render_svg("network", &state, "#f5f5f7"), EXPECTED);
    }

    #[test]
    fn a_gpu_strip_draws_a_second_series_and_an_extra_column() {
        const EXPECTED: &str = include_str!("../testdata/svg/gpu-loaded.svg");
        let state = object! {
            "history" => Value::List((0..POINTS).map(|i| Value::Float((i % 100) as f64)).collect()),
            "history2" => Value::List((0..POINTS).map(|i| Value::Float((i % 50) as f64)).collect()),
            "display" => Value::str("73%"),
            "extra" => Value::str("2.1G"),
            "active" => Value::Bool(true),
        };
        assert_eq!(render_svg("gpu", &state, "#f5f5f7"), EXPECTED);
    }

    #[test]
    fn an_unavailable_widget_renders_its_n_slash_a_at_the_dimmer_opacity() {
        const EXPECTED: &str = include_str!("../testdata/svg/disk-unavailable.svg");
        let mut state = Object::new();
        crate::state::mark_unavailable(
            "disk",
            &mut state,
            &crate::fault::Fault::os("no block device backing /"),
        );
        assert_eq!(render_svg("disk", &state, "#f5f5f7"), EXPECTED);
    }

    #[test]
    fn a_display_string_with_markup_in_it_is_escaped() {
        let state = object! { "display" => Value::str("<&\">") };
        assert!(render_svg("cpu", &state, "#f5f5f7").contains("&lt;&amp;&quot;&gt;"));
    }

    #[test]
    fn an_empty_extra_leaves_the_second_column_out_entirely() {
        let state = object! { "display" => Value::str("5%"), "extra" => Value::str("") };
        assert!(!render_svg("gpu", &state, "#f5f5f7").contains("class=\"secondary\""));
    }
}
