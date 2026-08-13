"""Python-vs-Rust differential tests.

This is a package rather than a loose directory so that unittest discovery
recurses into it: `unittest.defaultTestLoader.discover` skips a subdirectory that
has no __init__.py, and tests/run discovers with top_level_dir=tests/, which
makes the module here import as `differential.test_parity`. No change to
tests/run was needed.

Nothing in here imports the backend. tests/harness.py loads it in-process, which
is right for testing Python behaviour and impossible for testing a port; these
tests spawn both binaries as subprocesses and compare what came out.
"""
