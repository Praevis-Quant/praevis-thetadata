"""Smoke-test an installed praevis-thetadata wheel: import, version, and an
offline describe(). No network. Runs on the oldest supported Python (3.10).

Usage: python wheel_smoke.py <expected version>
"""
import sys

import praevis_thetadata as pt

expected = sys.argv[1]
assert pt.__version__ == expected, (pt.__version__, expected)
assert len(pt.endpoints()) == 100
query, parts = pt.describe("stock_history_eod", symbol="AAPL", start_date="2022-01-01", end_date="2024-03-01")
assert parts == 3 and "AAPL" in query, (query, parts)
_, parts = pt.describe("option_history_binomial_greeks_eod", symbol="SPY", expiration="*",
                       start_date="2026-11-09", end_date="2026-11-13")
assert parts == 5, parts
print(f"praevis_thetadata {pt.__version__} on Python {sys.version.split()[0]}: import and describe OK")
