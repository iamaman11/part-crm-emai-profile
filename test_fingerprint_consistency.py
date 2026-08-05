"""Retired legacy prototype kept only as a migration marker.

Fingerprint checks are implemented by tools/fingerprint_certify.py on disposable
clones and will move behind the Certification application port.
"""

import sys


def main() -> int:
    print(
        "test_fingerprint_consistency.py is retired: use the clone-only "
        "certification workflow. Never launch a legacy source profile directly.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
