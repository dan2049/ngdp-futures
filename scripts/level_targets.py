"""Regenerate and verify the NGDP Level Target table.

Target_n = 31,098.0 x 1.05^(n/4), n = quarters after Q3 2025, rounded to $0.1bn.
Base: BEA Q3 2025 Updated Estimate (Jan 22, 2026), current-dollar GDP, SAAR.
Run: python3 scripts/level_targets.py
"""
from decimal import Decimal, getcontext

getcontext().prec = 40
BASE = Decimal("31098.0")
LN_GROWTH = Decimal("1.05").ln()

year, quarter = 2025, 3
for n in range(41):
    target = BASE * (LN_GROWTH * n / 4).exp()
    tenths = int((target * 10).quantize(Decimal(1)))
    print(f"n={n:2d}  Q{quarter} {year}  ${target.quantize(Decimal('0.1')):>9,} bn  ({tenths} tenths)")
    quarter += 1
    if quarter == 5:
        quarter, year = 1, year + 1
