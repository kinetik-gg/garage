//! Network: which interface is "the network", and what its byte counters say.

use crate::fault::Fault;
use crate::files::{read_int, read_text};
use std::path::{Path, PathBuf};

/// The interface carrying the default route, from `/proc/net/route`.
///
/// Named interfaces drift (enp11s0 today, enp12s0 after a BIOS update) and a machine
/// with docker0, a VPN and wifi has half a dozen of them. The default route is the one
/// definition of "the network" that stays true.
///
/// # Errors
///
/// Returns [`Fault`] when the flags column of a candidate line is not hexadecimal --
/// Python's `int(field, 16)` raising `ValueError`.
pub(crate) fn default_interface() -> Result<Option<String>, Fault> {
    match read_text(Path::new("/proc/net/route")) {
        Some(text) if !text.is_empty() => pick(&text),
        _ => Ok(None),
    }
}

/// The scan itself, over the file's text.
///
/// A destination of all zeros and the `RTF_GATEWAY` bit set is the test, in that order:
/// the flags column is only parsed for a line that already looks like a default route,
/// which is the Python's short-circuit and not an optimisation. The first line is the
/// column header and is skipped.
fn pick(text: &str) -> Result<Option<String>, Fault> {
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (Some(name), Some(destination), Some(flags)) =
            (fields.first(), fields.get(1), fields.get(3))
        else {
            continue;
        };
        if *destination != "00000000" {
            continue;
        }
        let flags = i64::from_str_radix(flags, 16).map_err(|_| Fault::bad_hex(flags))?;
        if flags & 2 != 0 {
            return Ok(Some((*name).to_string()));
        }
    }
    Ok(None)
}

/// Bytes received and bytes sent, from the interface's own sysfs statistics.
pub(crate) fn counters(interface: &str) -> (Option<i64>, Option<i64>) {
    let root = statistics(interface);
    (
        read_int(&root.join("rx_bytes")),
        read_int(&root.join("tx_bytes")),
    )
}

fn statistics(interface: &str) -> PathBuf {
    PathBuf::from(format!("/sys/class/net/{interface}/statistics"))
}

#[cfg(test)]
mod tests {
    use super::{pick, statistics};
    use std::path::PathBuf;

    const ROUTE: &str = concat!(
        "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n",
        "docker0\t000011AC\t00000000\t0001\t0\t0\t0\t0000FFFF\t0\t0\t0\n",
        "enp11s0\t0000FEA9\t00000000\t0001\t0\t0\t1000\t0000FFFF\t0\t0\t0\n",
        "enp11s0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n",
    );

    #[test]
    fn the_default_route_is_the_one_with_a_zero_destination_and_the_gateway_bit() {
        assert_eq!(pick(ROUTE), Ok(Some("enp11s0".to_string())));
    }

    #[test]
    fn the_header_line_is_never_a_candidate() {
        assert_eq!(pick("Iface\tDestination\tGateway \tFlags\n"), Ok(None));
    }

    #[test]
    fn a_zero_destination_without_the_gateway_bit_is_not_the_default_route() {
        let text = concat!(
            "Iface\tDestination\tGateway \tFlags\n",
            "wg0\t00000000\t00000000\t0001\t0\t0\t0\n",
        );
        assert_eq!(pick(text), Ok(None));
    }

    #[test]
    fn a_machine_with_no_route_at_all_reports_nothing() {
        assert_eq!(pick(""), Ok(None));
        assert_eq!(pick("Iface\tDestination\n"), Ok(None));
    }

    #[test]
    fn a_short_line_is_skipped_rather_than_indexed_past_the_end() {
        assert_eq!(pick("Iface\tDestination\nlo\t00000000\n"), Ok(None));
    }

    #[test]
    fn a_non_hexadecimal_flags_column_on_a_candidate_line_is_a_value_error() {
        let text = concat!(
            "Iface\tDestination\tGateway \tFlags\n",
            "eth0\t00000000\t00000000\tzzzz\t0\t0\t0\n",
        );
        assert_eq!(
            pick(text).expect_err("not hexadecimal").to_string(),
            "invalid literal for int() with base 16: 'zzzz'"
        );
    }

    #[test]
    fn a_non_hexadecimal_flags_column_elsewhere_is_never_parsed() {
        // The Python's `and` short-circuits, so a garbage flags column on a line that
        // is not a default route candidate never reaches int().
        let text = concat!(
            "Iface\tDestination\tGateway \tFlags\n",
            "eth0\t0000FEA9\t00000000\tzzzz\t0\t0\t0\n",
            "eth1\t00000000\t0101A8C0\t0003\t0\t0\t0\n",
        );
        assert_eq!(pick(text), Ok(Some("eth1".to_string())));
    }

    #[test]
    fn counters_come_from_the_interfaces_own_statistics_directory() {
        assert_eq!(
            statistics("enp11s0"),
            PathBuf::from("/sys/class/net/enp11s0/statistics")
        );
    }
}
