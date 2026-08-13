//! `garage update`: pull, sweep dead links, then delegate to `bootstrap.sh`.
//!
//! # The delegate-to-bootstrap banner
//!
//! Design: pull, sweep, then delegate. Not a native reimplementation, and the choice is not
//! close. What an update has to do on rolling Arch is make this machine match the checkout
//! again: install packages the list has gained, enable units it has gained, write per-user
//! files that are new, put every link back, and leave the user's own files alone.
//! `bootstrap.sh` already does all of that, idempotently and by design -- its freshness gate
//! short-circuits on an install it can already see, packages go in with `--needed`,
//! `systemctl enable` is bookkeeping, generated files are written only when absent, and every
//! mutating call goes through one `run()` chokepoint a `--dry-run` flag turns off. It is also
//! the only path exercised by the clean-install rehearsal.
//!
//! A native implementation would have to duplicate the part that is genuinely hard: the
//! pre-stow scan that classifies every managed path, moves real files into a timestamped
//! backup, and deletes links from a moved checkout. Without it, `stow --restow` aborts on the
//! first tracked file that has become a real file and leaves a half-linked home. With it, two
//! implementations of the same careful logic drift apart, and the one this crate would carry
//! is the one no rehearsal covers.
//!
//! What delegation costs, honestly: `bootstrap.sh` prints for a TTY -- steps, warnings, a
//! summary -- which is acceptable here precisely because `garage update` is a TTY command
//! too, and a re-run includes `sudo pacman -Syu`, so `update` asks for sudo and can upgrade
//! the whole system, which is stated up front rather than designed around, since installing
//! newly listed packages without a full upgrade first is an unsupported partial upgrade on
//! Arch.
//!
//! Two things `update` keeps for itself, because `bootstrap.sh` cannot answer them: the
//! dead-link sweep ([`crate::doctor::stow`]'s `dangling_repo_links()`), which needs to know
//! what the checkout no longer ships -- the one thing a scan of the *current* manifest cannot
//! see -- and the plugin decision, since `bootstrap.sh` deploys unconditionally (right for an
//! install, wasteful for an update that rebuilds an unmoved ABI), so `update` compares the
//! running ABI against what is deployed itself and passes
//! `GARAGE_SKIP_PLUGIN_DEPLOY=1` so `bootstrap.sh` does not do it twice.
//!
//! Doc-only: takes `argv` and returns an exit code, prints lines rather than the JSON
//! response envelope, and streams `bootstrap.sh`'s own output to the terminal rather than
//! capturing it -- the same reason `Runner::run_streamed` exists rather than `Runner::run`.
