# integ-w6t-rv-burst: who owns criterion 6's ingest → WS tail

`frontier-node/crates/herald/tests/burst.rs` (integ-W6t review): R5's recording (`integ-w6t-tri-20x/verify/input.json.gz`) folded up to slot 2040, then slots 2041–2300 replayed at 400 ms a slot by a herald in the test process on 127.0.0.1:41140, while `frontier-viewers` runs as a separate process (4,000 pollers, think 5 s; 1,000 WS viewers on every ring and every opened province). 247 transactions, 72 in slots 2075–2076 (the `ticket` hold's release), 1,409,055 WS messages. Same herald build (`dcfece9`: messages carry `t` and `s`) for both runs; only the generator differs.

```
HERALD_BURST_INPUT=.../integ-w6t-tri-20x/verify/input.json.gz HERALD_BURST_PORT=41140 \
HERALD_BURST_VIEWERS_BIN=<generator> HERALD_BURST_REPORT=<file> \
cargo test --release -p herald --test burst -- --ignored --nocapture
```

| generator | ingest → WS p50 / p99 upper / max | herald `s − t` p50 / p99 upper / max | delivery p50 / p99 upper / max | file p99 (answered) | load 1 min (start → end) |
|---|---|---|---|---|---|
| `aa87235` (pre-fix) | 1,376 / **3,277** / 3,324 ms | – | – | 49.2 ms | 5.15 → 4.48 (2026-09-30 10:14–10:16 JST) |
| `dcfece9` (fixed) | 590 / **754** / 732 ms | 557 / 688 / 672 ms | 12.8 / 86.0 / 124 ms | 8.2 ms (9.2 ms) | 4.44 → 3.94 (10:17–10:19 JST) |

The full generator reports are `burst-old-generator.json` and `burst-new-generator.json`.
