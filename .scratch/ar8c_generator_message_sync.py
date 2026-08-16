#!/usr/bin/env python3
from pathlib import Path

path = Path('scripts/generate-architecture-inventory.py')
text = path.read_text(encoding='utf-8')
replacements = {
    'Architecture inventory active AR-8 / current AR-8B negative self-test passed.': 'Architecture inventory active AR-8 / AR-8A+B accepted / current AR-8C negative self-test passed.',
    'Architecture inventory projects active AR-8 with AR-8A accepted and AR-8B current.': 'Architecture inventory projects active AR-8 with AR-8A+B accepted and AR-8C current.',
}
for old, new in replacements.items():
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'expected exactly one occurrence of {old!r}, observed {count}')
    text = text.replace(old, new, 1)
path.write_text(text, encoding='utf-8')
