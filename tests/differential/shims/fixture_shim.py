"""Template for every PATH shim except luac. Materialized, not executed here.

runner.materialize_shims() reads this file, prepends `#!<absolute python3>` and
writes one copy per shimmed binary name into the scratch shim directory. The
shebang has to be absolute because the child's PATH is the shim directory and
nothing else: `#!/usr/bin/env python3` would send env looking for python3 on a
PATH that contains only shims, and every shim would fail to start.

Two jobs, in this order:

  1. Append the invocation to $GARAGE_DIFF_TRACE. The trace is the comparison
     surface that matters most -- half of what the backend *does* is which
     external commands it runs, in which order, with which arguments. A port
     that produces byte-identical stdout while never calling `hyprctl reload`
     has not ported anything.
  2. Answer from the fixture table at $GARAGE_DIFF_FIXTURES: exact match on the
     joined argv first, then the longest key that the joined argv starts with.
     Missing key means empty stdout, empty stderr, exit 0 -- the "command exists
     and had nothing to say" answer, which is what most of these calls get.
"""

import json
import os
import shlex
import sys


def joined_argv() -> str:
    # basename, so the trace does not carry the scratch shim directory into
    # every line. The backend asked for "hyprctl"; that is what gets recorded.
    return shlex.join([os.path.basename(sys.argv[0])] + sys.argv[1:])


def record(line: str) -> None:
    trace = os.environ.get("GARAGE_DIFF_TRACE")
    if not trace:
        return
    # One open-append-close per invocation, with a single write of one short
    # line: O_APPEND writes below PIPE_BUF are atomic, so concurrent shims
    # cannot interleave halves of each other's lines.
    with open(trace, "a", encoding="utf-8") as handle:
        handle.write(line + "\n")


def lookup(line: str) -> dict:
    path = os.environ.get("GARAGE_DIFF_FIXTURES")
    if not path or not os.path.exists(path):
        return {}
    with open(path, encoding="utf-8") as handle:
        table = json.load(handle)
    if line in table:
        return table[line]
    best = ""
    for key in table:
        if line.startswith(key) and len(key) > len(best):
            best = key
    return table.get(best, {}) if best else {}


def main() -> int:
    line = joined_argv()
    record(line)
    answer = lookup(line)
    sys.stdout.write(str(answer.get("stdout", "")))
    sys.stderr.write(str(answer.get("stderr", "")))
    return int(answer.get("returncode", 0))


if __name__ == "__main__":
    sys.exit(main())
