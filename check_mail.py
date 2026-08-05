"""Retired legacy prototype kept only as a migration marker.

Mailbox operations will be implemented through provider adapters and the
versioned application boundary described in IMPLEMENTATION_PLAN.md.
"""

import sys


def main() -> int:
    print(
        "check_mail.py is retired: it launched mutable profiles directly and "
        "cannot be used safely. Use the future Mailbox Operations application API.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
