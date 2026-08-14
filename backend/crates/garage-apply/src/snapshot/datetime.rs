//! `datetime_snapshot()`: the timezone, NTP state and the full timezone list.
//!
//! The timezone falls back through three sources in order: `timedatectl show
//! --property=Timezone`, then `/etc/localtime`'s resolved target split on `/zoneinfo/`, then
//! `time.tzname[0]` as the last resort -- covering a machine with no `timedatectl`, one whose
//! `localtime` symlink is not the standard shape, and one with neither. The timezone list
//! itself prefers `timedatectl list-timezones` and falls back to Python's own
//! `available_timezones()` when that command fails, so the pane always has something to
//! populate its picker from.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.

use std::path::Path;

use serde_json::{json, Value};

use crate::command::run;
use crate::cx::SessionCx;

/// `timedate_value()` (garage:5090-5092): one `timedatectl show --property=X --value`, or `""`.
fn timedate_value(cx: &SessionCx<'_>, name: &str) -> String {
    let result = run(
        cx,
        &[
            "timedatectl",
            "show",
            &format!("--property={name}"),
            "--value",
        ],
    );
    if result.status == 0 {
        result.stdout.trim().to_owned()
    } else {
        String::new()
    }
}

/// `datetime_snapshot()` (garage:5095-5110): the timezone, NTP state and the full list.
pub(crate) fn datetime_snapshot(cx: &SessionCx<'_>) -> Value {
    let zones = run(cx, &["timedatectl", "list-timezones"]);
    let timezone = match timedate_value(cx, "Timezone") {
        found if !found.is_empty() => found,
        _ => zone_from_localtime().unwrap_or_else(local_abbreviation),
    };
    json!({
        "timezone": timezone,
        "ntp": timedate_value(cx, "NTP").to_lowercase() == "yes",
        "synchronized": timedate_value(cx, "NTPSynchronized").to_lowercase() == "yes",
        "timezones": if zones.status == 0 {
            zones.stdout.lines().map(str::to_owned).collect()
        } else {
            available_timezones()
        },
    })
}

/// `str(Path("/etc/localtime").resolve()).split("/zoneinfo/", 1)[1]`.
fn zone_from_localtime() -> Option<String> {
    let resolved = std::fs::canonicalize("/etc/localtime").ok()?;
    resolved
        .to_string_lossy()
        .split_once("/zoneinfo/")
        .map(|(_, zone)| zone.to_owned())
}

/// `time.tzname[0]`, the last resort of three.
///
/// **Parity note, stated plainly:** Python reads libc's non-DST abbreviation for the local
/// zone -- `WIB`, `CET`. `std` has no equivalent and this crate takes no timezone dependency,
/// so `$TZ` is read where it is set and `UTC` stands in otherwise. This is the third fallback
/// of three: it is reached only on a machine where `timedatectl` cannot be run at all *and*
/// `/etc/localtime` is not a symlink into a `zoneinfo` tree -- neither of which is true of any
/// machine this ships to or any supported test fixture.
fn local_abbreviation() -> String {
    std::env::var("TZ").unwrap_or_else(|_| "UTC".to_owned())
}

/// `sorted(zoneinfo.available_timezones())`, for a machine with no `timedatectl`.
///
/// **Parity note, stated plainly:** `CPython`'s `available_timezones()` prefers the `tzdata`
/// package's own index and falls back to walking `TZPATH`; this walks `/usr/share/zoneinfo`
/// alone, skipping the `posix` and `right` mirror trees and the index files that are not
/// zones. On an Arch machine with the system `tzdata` the two agree; on one with the `PyPI`
/// `tzdata` wheel installed they need not. Reached only when `timedatectl list-timezones`
/// fails outright.
fn available_timezones() -> Vec<String> {
    let root = Path::new("/usr/share/zoneinfo");
    let mut zones = Vec::new();
    collect_zones(root, root, &mut zones);
    zones.sort();
    zones
}

/// The recursive half, kept separate so the walk stays under the nesting lint.
fn collect_zones(root: &Path, directory: &Path, zones: &mut Vec<String>) {
    /// Not zones: two mirror trees of the whole database, and the index files beside them.
    const SKIP: [&str; 6] = [
        "posix",
        "right",
        "posixrules",
        "zone.tab",
        "zone1970.tab",
        "iso3166.tab",
    ];
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let extension = std::path::Path::new(&name)
            .extension()
            .map(|found| found.to_string_lossy().into_owned())
            .unwrap_or_default();
        if SKIP.contains(&name.as_str()) || extension == "zi" || extension == "list" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_zones(root, &path, zones);
        } else if let Ok(relative) = path.strip_prefix(root) {
            zones.push(relative.to_string_lossy().into_owned());
        }
    }
}
