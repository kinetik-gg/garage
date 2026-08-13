//! CLI surface for reconcile's human/JSON hybrid output.

use garage_core::paths::Paths;
use serde_json::Value;

use crate::response::emit;

/// Dispatch `garage reconcile [--dry-run] [--prune] [--json]`.
pub(crate) fn run(paths: &Paths, argv: &[String]) -> u8 {
    let arguments = argv.get(2..).unwrap_or_default();
    let json = arguments.iter().any(|argument| argument == "--json");
    let outcome = options(arguments)
        .map_err(str::to_owned)
        .and_then(|options| {
            garage_reconcile::reconcile(paths, options).map_err(|error| error.to_string())
        });
    match outcome {
        Ok(report) => emit_report(&report, json),
        Err(error) => emit_error(&error, json),
    }
}

fn options(arguments: &[String]) -> Result<garage_reconcile::Options, &'static str> {
    let mut options = garage_reconcile::Options::default();
    for argument in arguments {
        match argument.as_str() {
            "--dry-run" => options.dry_run = true,
            "--prune" => options.prune = true,
            "--json" => (),
            _ => return Err("Usage: garage reconcile [--dry-run] [--prune] [--json]"),
        }
    }
    Ok(options)
}

fn emit_report(report: &garage_reconcile::Report, json: bool) -> u8 {
    if json {
        match serde_json::to_value(report) {
            Ok(value) => emit(&value, ""),
            Err(error) => {
                emit(&Value::Null, &error.to_string());
                return 1;
            }
        }
    } else {
        print!("{}", garage_reconcile::render_human(report));
    }
    0
}

fn emit_error(error: &str, json: bool) -> u8 {
    if json {
        emit(&Value::Null, error);
    } else {
        eprintln!("garage reconcile: {error}");
    }
    1
}

#[cfg(test)]
mod tests {
    use super::options;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    #[test]
    fn flags_compose_and_an_unknown_argument_is_the_commands_usage() {
        let parsed =
            options(&args(&["--json", "--prune", "--dry-run"])).expect("all documented flags");
        assert!(parsed.dry_run);
        assert!(parsed.prune);
        assert_eq!(
            options(&args(&["--force"]))
                .expect_err("unknown flag")
                .to_string(),
            "Usage: garage reconcile [--dry-run] [--prune] [--json]"
        );
    }
}
