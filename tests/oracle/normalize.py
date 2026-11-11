"""Normalize the pinned Solidity oracle output into the Rust regression fixture."""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REVISION = "bea7f6d1c11cd5dc4f0d5151ffa37a1c239feee6"
actual = subprocess.check_output(
    ["git", "-C", str(ROOT / "lib/v4-core"), "rev-parse", "HEAD"], text=True
).strip()
if actual != REVISION:
    raise SystemExit(f"Expected v4-core {REVISION}, found {actual}")
raw = json.loads((ROOT / "vectors.json").read_text())
if raw["core_revision"] != actual:
    raise SystemExit("Oracle metadata does not match the checked-out core revision")
keys = sorted((key for key in raw if key.startswith("case_")), key=lambda key: int(key[5:]))
cases = []
for key in keys:
    row = json.loads(raw[key]) if isinstance(raw[key], str) else raw[key]
    if not isinstance(row, dict):
        raise SystemExit(f"Invalid oracle record {key}")
    cases.append({"id": key, **row})
if len(cases) != 40:
    raise SystemExit(f"Expected 40 cases, found {len(cases)}")
fixture = {
    "core_revision": actual,
    "source": "Uniswap/v4-core Pool.swap; tests/oracle contains regeneration harness",
    "initial_sqrt_price": str(1 << 96),
    "initial_tick": 0,
    "tick_spacing": 10,
    "cases": cases,
}
(ROOT.parent / "fixtures/v4_core.json").write_text(json.dumps(fixture, indent=2) + "\n")
print(f"Wrote {len(keys)} v4-core cases")
