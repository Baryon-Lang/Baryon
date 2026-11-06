"""Checks that every file named on the command line is valid Python 3.12 syntax.

Run by `cargo xtask check-examples`. Prints one line per invalid file and exits with status 1 if
any file is invalid, so every problem is reported in one run.
"""

import ast
import sys

if sys.version_info < (3, 12):
    sys.exit(f"Python 3.12 or newer is required to check examples; found {sys.version.split()[0]}")

failed = False
for path in sys.argv[1:]:
    with open(path, encoding="utf-8") as file:
        source = file.read()
    try:
        ast.parse(source, path, feature_version=(3, 12))
    except SyntaxError as error:
        print(f"{path}:{error.lineno}: {error.msg}", file=sys.stderr)
        failed = True

sys.exit(1 if failed else 0)
