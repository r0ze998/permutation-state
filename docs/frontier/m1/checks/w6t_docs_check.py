#!/usr/bin/env python3
"""Docs check for the w6-s7 triage amendments (contract v1.12 §28, W6-E / fix unit U5).

Two modes:

  python3 docs/frontier/m1/checks/w6t_docs_check.py [--root REPO]
      The documentation checks: the contract carries v1.12 and §28 with a row
      for every triage change, the normative text is changed in place (§3.2,
      §5.5, §5.11, §6, §8.2, §8.3, §8.5, §8.6, §10.2, §12, §13.4, §13.5, §15),
      DECISIONS part S has one entry per row with an evidence pointer, DESIGN
      has the season-end Depart line and the shielded-march note, and the
      keeper guide documents `backup_delay_slots`, the 409 answers and the
      status endpoint. The code cross-checks are printed as PENDING.

  python3 docs/frontier/m1/checks/w6t_docs_check.py --final [--root REPO]
      Also fails on every PENDING item: a value left for the integrator to fill
      at merge (the marker "⟨pending"), or a code fact the text states that the
      tree does not have yet (the U1-U4 merges: the Depart bound, the close
      limits in budgets.json, the keeper's 409 answers and `backup_delay_slots`,
      the verifier's `ArrivalAfterEnd` and T24, the stack and viewer flags).
      Run it on the integration tree after the U5 merge.

Exit 0 when every check passes, 1 otherwise. Standard library only.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

PENDING = "⟨pending"


def section(text: str, start: str, end: str | None) -> str:
    """The text from the first line starting with `start` up to the next line
    starting with `end` (or the end of the text)."""
    lines = text.splitlines()
    out, on = [], False
    for ln in lines:
        if not on and ln.startswith(start):
            on = True
        elif on and end is not None and ln.startswith(end):
            break
        if on:
            out.append(ln)
    return "\n".join(out)


class Checker:
    def __init__(self, final: bool):
        self.final = final
        self.fails: list[str] = []
        self.pending: list[str] = []
        self.passes = 0

    def ok(self, cond: bool, what: str) -> None:
        if cond:
            self.passes += 1
        else:
            self.fails.append(what)

    def code(self, cond: bool, what: str) -> None:
        """A code fact the docs state; PENDING until the fix units merge."""
        if cond:
            self.passes += 1
        elif self.final:
            self.fails.append("code: " + what)
        else:
            self.pending.append("code: " + what)

    def has(self, text: str, needles: list[str], where: str) -> None:
        for n in needles:
            self.ok(n in text, f"{where}: missing {n!r}")


def read(p: Path) -> str:
    try:
        return p.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(Path(__file__).resolve().parents[4]))
    ap.add_argument("--final", action="store_true")
    a = ap.parse_args()
    root = Path(a.root)
    c = Checker(a.final)

    docs = root / "docs/frontier"
    contract = read(docs / "m1/M1-CONTRACT.md")
    decisions = read(docs / "DECISIONS.md")
    design = read(docs / "DESIGN.md")
    guide = read(docs / "m1/RUN-A-KEEPER.md")
    notes = read(docs / "m1/W6T-5-NOTES.md")

    # ---- contract: version line and §28 --------------------------------
    header = next((ln for ln in contract.splitlines() if ln.startswith("**Version")), "")
    # v1.12 or a later version that keeps the v1.12 entry (integ-W6t review: v1.13).
    c.ok(
        header.startswith("**Version v1.12**") or header.startswith("**Version v1.13**"),
        "contract: header is not **Version v1.12** (or v1.13)",
    )
    c.ok("§28 lists each change" in header, "contract: header does not point at §28")
    s28 = section(contract, "## 28.", "## 29.")
    c.ok(s28.startswith("## 28. Amendments v1.12"), "contract: no '## 28. Amendments v1.12'")
    rows = [ln for ln in s28.splitlines() if ln.startswith("| ") and not ln.startswith("| Section") and not ln.startswith("|---")]
    first_cells = [r.split("|")[1].strip() for r in rows]
    for sec in ["§5.11 Depart", "§6", "§5.5, §10.2", "§8.2", "§8.3", "§8.5", "§8.6", "§13.4", "§13.5", "§12", "§3.2", "§15"]:
        c.ok(any(fc.startswith(sec) for fc in first_cells), f"§28: no row for {sec}")
    c.has(s28, [
        "min(now_bell + 72, end_bell − 1)", "ArrivalBell", "end_bell − 3",
        "CAMP", "spawn", "clear",
        "CloseArrivalDay", "CloseArrivalSlot",
        "anchors_plan", "409", "Shielded", "exceeded CUs meter", "Dead", "backup_delay_slots", "skip_target",
        "unrevealed_by_rule", "reason", "(host, arrive)", "ArrivalAfterEnd", "T24", "V11",
        "A1", "A2", "A3",
        "--season-end-at-play-end", "--chaos-force", "not exit-grade",
        "RULESET_HASH", "72c6b583",
        "O-M1-25", "O-M1-26", "O-M1-27",
    ], "§28")
    for r in rows:
        cells = [x.strip() for x in r.split("|")[1:-1]]
        c.ok(len(cells) == 3 and all(cells), f"§28: row without three filled cells: {r[:60]!r}")

    # ---- contract: normative text in place -----------------------------
    s32 = section(contract, "### 3.2", "### 3.3")
    c.has(s32, ["v1.12", "RULESET_HASH", "72c6b5835ded6418ed98b0c00b2ae45ce4c4b082d9614447dbce2c9d2e654bd9"], "§3.2")
    s55 = section(contract, "### 5.5", "### 5.6")
    for tag in ("0x65 | CloseArrivalDay", "0x66 | CloseArrivalSlot"):
        row = next((ln for ln in s55.splitlines() if tag in ln), "")
        c.ok("v1.12" in row, f"§5.5: row {tag!r} has no v1.12 limit note")
    s511 = section(contract, "### 5.11", "### 5.12")
    step4 = next((ln for ln in s511.splitlines() if ln.startswith("4. `arrive_bell")), "")
    c.ok("min(now_bell + 72, end_bell − 1)" in step4, "§5.11 Depart step 4: no end_bell bound")
    rfi = next((ln for ln in s511.splitlines() if ln.startswith("**ResolveFromInputs")), "")
    c.ok("`CAMP` (spawn)" in rfi and "`CAMP` (clear" in rfi, "§5.11 ResolveFromInputs: log order spawn, clear, CLASH not stated")
    s6 = section(contract, "## 6.", "## 7.")
    row43 = next((ln for ln in s6.splitlines() if ln.startswith("| 43 | CAMP")), "")
    c.ok("clear" in row43 and "spawned" in row43, "§6 row 43 CAMP: the clear record not documented")
    s82 = section(contract, "### 8.2", "### 8.3")
    c.has(s82, ["end_bell − 1", 'code: "Shielded"', 'code: "ArrivalBell"', "backup_delay_slots",
                "exceeded CUs meter", "within 1 s", "`duties`"], "§8.2")
    s83 = section(contract, "### 8.3", "### 8.4")
    c.has(s83, ["409", "ArrivalBell"], "§8.3")
    s85 = section(contract, "### 8.5", "### 8.6")
    c.has(s85, ["ArrivalAfterEnd", "**T24**", "reason", "(host, arrive)", "spawned earlier in the same transaction"], "§8.5")
    s86 = section(contract, "### 8.6", "### 8.7")
    c.has(s86, ["end_bell − 1", "own Holding", "`accepted`"], "§8.6")
    s102 = section(contract, "### 10.2", "### 10.3")
    c.ok("v1.12" in s102, "§10.2: no v1.12 close-limit note")
    s12 = section(contract, "## 12.", "## 13.")
    c.has(s12, ["--season-end-at-play-end", "--chaos-force", "--retry-budget-ms", "not exit-grade"], "§12 Gate W6")
    s134 = section(contract, "### 13.4", "### 13.5")
    c.has(s134, ["(A1, v1.12)", "(A2, v1.12)", "(A3, v1.12)", "no GATHER or CLASH", "unrevealed_by_rule", "stale keep-alive"], "§13.4")
    s135 = section(contract, "### 13.5", "### 13.6")
    c.ok("T24" in s135, "§13.5: the tamper count does not include T24")
    s15 = section(contract, "## 15.", "## 16.")
    c.has(s15, ["O-M1-25", "O-M1-26", "O-M1-27", "fmarch.mjs", "screens/march.mjs", "screens/holding.mjs",
                "fland.mjs", "session.mjs"], "§15")

    # ---- DECISIONS part S ----------------------------------------------
    partS = section(decisions, "## S.", "## E.")
    c.ok(partS.startswith("## S."), "DECISIONS: no part S")
    srows = [ln for ln in partS.splitlines() if re.match(r"^\| S\d+", ln)]
    c.ok(len(srows) >= 14, f"DECISIONS part S: {len(srows)} entries, expected ≥ 14 (one per §28 row)")
    for r in srows:
        cells = [x.strip() for x in r.split("|")[1:-1]]
        c.ok(len(cells) == 4 and "`" in cells[3], f"DECISIONS {cells[0] if cells else r[:20]}: no evidence pointer")
    for sec in ["§5.11", "§6", "§5.5", "§8.2", "§8.3", "§8.5", "§8.6", "§13.4", "§12", "§3.2", "§15"]:
        c.ok(any(sec in r.split("|")[1] for r in srows), f"DECISIONS part S: no entry for {sec}")
    log = section(decisions, "## E.", None)
    c.ok("| v1.12" in log, "DECISIONS change log: no v1.12 row")

    # ---- DESIGN -----------------------------------------------------------
    c.ok("end_bell − 3" in section(design, "### 2.6", "### 2.7"), "DESIGN §2.6: no season-end Depart line")
    s62 = section(design, "### 6.2", "### 6.3")
    notrev = next((ln for ln in s62.splitlines() if ln.startswith("- **Not revealed by the reveal close:**")), "")
    c.ok("Shielded" in notrev and "routed by rule" in notrev and "clients prevent" in notrev,
         "DESIGN §6.2: no shielded-march note (routed by rule, clients prevent it)")
    c.ok("end_bell − 1" in s62, "DESIGN §6.2: Depart arrival bound not stated")
    c.ok("## 23. M1 w6-s7 triage amendments" in design, "DESIGN: no §23 listing the triage changes")

    # ---- keeper guide -------------------------------------------------------
    c.has(guide, ["backup_delay_slots", '409 {"error": "Shielded", "code": "Shielded"', '409 {"error": "ArrivalBell", "code": "ArrivalBell"',
                  "within 1 s", "`duties`", "contract v1.12"], "RUN-A-KEEPER")
    c.ok("The reveal-pool floor is computed at the Reveal budget (26,000 CU)" not in guide,
         "RUN-A-KEEPER §9: the fixed reveal-floor gap is still listed")

    # ---- notes --------------------------------------------------------------
    c.ok(bool(notes), "docs/frontier/m1/W6T-5-NOTES.md missing")

    # ---- pending values and code facts (PENDING unless --final) ----------------
    owned = {"M1-CONTRACT.md": contract, "DECISIONS.md": decisions, "DESIGN.md": design, "RUN-A-KEEPER.md": guide}
    for name, t in owned.items():
        n = t.count(PENDING)
        c.code(n == 0, f"{name}: {n} value(s) marked {PENDING}… left for the integrator")

    host = read(root / "permutation-frontier/src/proc/host.rs")
    c.code("end_bell" in host, "U1: Depart step 4 refuses arrive_bell ≥ end_bell (permutation-frontier/src/proc/host.rs)")
    lim: dict = {}

    def rows_of(v) -> None:
        """Every {tag, cu_limit} object anywhere in budgets.json."""
        if isinstance(v, dict):
            if "tag" in v and "cu_limit" in v:
                lim[v["tag"]] = v["cu_limit"]
            for x in v.values():
                rows_of(x)
        elif isinstance(v, list):
            for x in v:
                rows_of(x)

    try:
        rows_of(json.loads(read(root / "frontier-abi/vectors/budgets.json") or "{}"))
    except ValueError:
        pass
    c.ok(lim.get(101) is not None, "budgets.json: no CloseArrivalDay (tag 101) row found")
    m = re.search(r"CloseArrivalDay and CloseArrivalSlot limit \*\*([\d,]+)\*\*", s28)
    want = int(m.group(1).replace(",", "")) if m else None
    c.ok(want is not None, "§28: the close limit is not stated as 'CloseArrivalDay and CloseArrivalSlot limit **N**'")
    c.code(want is not None and lim.get(101) == want and lim.get(102) == want,
           f"U1: budgets.json cu_limit of tags 0x65/0x66 = {lim.get(101)}/{lim.get(102)}, §28 says {want}")
    keeper = root / "frontier-node/crates/keeper/src"
    c.code("backup_delay_slots" in read(keeper / "config.rs"), "U2: keeper.toml key backup_delay_slots (keeper/src/config.rs)")
    ra = read(keeper / "reveal_accept.rs")
    c.code('"Shielded"' in ra and '"ArrivalBell"' in ra, "U2: reveal_accept answers 409 Shielded / ArrivalBell")
    vsrc = "".join(read(p) for p in sorted((root / "frontier-node/crates/verify/src").rglob("*.rs")))
    c.code("ArrivalAfterEnd" in vsrc and '"T24"' in vsrc, "U3: verifier ArrivalAfterEnd and tamper class T24")
    ssrc = "".join(read(p) for p in sorted((root / "frontier-node/crates/stack/src").rglob("*.rs")))
    c.code("season-end-at-play-end" in ssrc and "chaos-force" in ssrc, "U4: stack flags --season-end-at-play-end, --chaos-force")
    vw = read(root / "frontier-node/crates/herald/src/bin/viewers.rs") + read(root / "frontier-node/crates/herald/src/viewers.rs")
    c.code("retry-budget-ms" in vw and "follow-status" in vw, "U3: viewer flags --retry-budget-ms, --follow-status")
    shas = set(re.findall(r"\b[0-9a-f]{64}\b", s32))
    c.code(any(not s.startswith("072b1205") and not s.startswith("72c6b583") for s in shas),
           "U1: §3.2 names the new release .so sha256")

    for f in c.fails:
        print("FAIL", f)
    for p in c.pending:
        print("PENDING", p)
    print(f"w6t docs check: {c.passes} pass, {len(c.fails)} fail, {len(c.pending)} pending"
          + (" (--final)" if a.final else ""))
    return 1 if c.fails else 0


if __name__ == "__main__":
    sys.exit(main())
