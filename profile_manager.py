"""Retired legacy prototype kept only as a migration marker.

Profile execution is owned by the future Profile Bridge. Direct launch and
automatic Firefox lock deletion violate the target lifecycle invariants.
"""

import sys


def main() -> int:
    print(
        "profile_manager.py is retired: use the future Profile Bridge lifecycle. "
        "Legacy source profiles must only be inspected or migrated through clones.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
