"""Template for the one shim that is not a fake: luac.

Every other external command is faked because its output is an input to the
backend's decisions and we want to choose that input. luac is different -- it is
a *checker*, and what it says about a generated Lua fragment is a fact about the
fragment, not about the machine. Faking it would mean both backends could emit
syntactically broken Lua and the harness would call that parity. So this shim
records the call in the trace like all the others and then execs the real luac
found at harness setup, by absolute path, because the child's PATH is
shims-only.

If the machine has no luac, runner.materialize_shims() does not write this shim
at all. That is deliberate: the backend gates its Lua checks on
`shutil.which("luac")`, so an absent shim makes both backends skip the check
identically, which is still parity. A stub that pretended luac existed and
always passed would be worse than no shim -- it would claim a check ran.
"""

import os
import shlex
import sys


LUAC = "@LUAC@"


def main() -> None:
    line = shlex.join([os.path.basename(sys.argv[0])] + sys.argv[1:])
    trace = os.environ.get("GARAGE_DIFF_TRACE")
    if trace:
        with open(trace, "a", encoding="utf-8") as handle:
            handle.write(line + "\n")
    # execv, not subprocess: the real luac's exit status and streams become this
    # process's, with nothing in between to get them wrong.
    os.execv(LUAC, ["luac"] + sys.argv[1:])


if __name__ == "__main__":
    main()
