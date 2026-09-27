# SP-V2: Frontier program spikes redone on SBPF v2 with the corrected account designs

> **Correction after review (m0c, 2026-09-27).** The review found that when THE anchor is absent, Reveal treated its window as open **with no time limit**, so an anchor closed by `ArchiveAnchors` 48 h after its bell would reopen the window for an already-resolved bell. **Confirmed**: with the fix below disabled, test `t9` shows a Reveal landing 30 days after A (mutation check). **Fix, in this lab:**
> - New instruction `CloseAnchor {region, bell, mode}` (10): the safety-relevant part of `ArchiveAnchors`. Allowed once `Clock ≥ A + 48 h`; it sets the bell's bit in the region-day **AnchorArchive tombstone** (`aa ‖ hex(region, day)`, 64 B, created pre-funding-safely) and then closes THE anchor (data freed, assigned back to the System program, rent to the payer).
> - `RevealSlot` and `PostAnchor` take the archive account (one read-only key, +32 B): with THE anchor absent, a tombstoned bell is refused with the new code `Custom(18)` (Archived). GatherClash, PostSeed and ProveBadSeal need the anchor itself and refuse with `Custom(8)`.
> - Why a tombstone and not a time limit (`Clock < time(T(b)) + 48 h`): a drand outage longer than 48 h must only delay (A4); with a clock limit it would refuse the late anchor and strand the bell.
> - **Test `t9_archived_anchor_does_not_reopen_the_window` [measured, v2 and v0]:** close at A + 48 h − 1 s refused (13); close at A + 48 h over a **pre-funded** archive address lands (6,201 CU, 361 B); the anchor is absent afterwards; Reveal at A + 48 h + 5 s and at A + 30 days refused (18); a second PostAnchor refused (18); PostSeed, GatherClash and a second close refused (8); control: a never-anchored bell of the same region-day is still open 30 days later; a forged archive address refused (9).
> - **Numbers that moved** (full re-run `results/run-log-m0c.txt`, 12 tests per architecture, all pass; the pre-fix JSONs are in `results/pre-m0c/`): PostAnchor **331,810–335,542 CU, 709 B** (was 331,253–334,985, 676 B), repeat no-op 1,752 CU; the ArrivalSlot-creating part of Reveal **6,603 CU, 423 B** (was 6,356, 390 B); resolves ±2 CU. Everything else is unchanged.
> - **Design impact 6** stands, and now has a rule for the archive: the predictable address plus the tombstone.

- **Date:** 2026-09-27
- **Lab:** `scratchpad/frontier/m0b/spikes/SP-V2/`. It is a new lab. The M0 labs S-BEACON, S-SIZE-JOIN and S-TLOCK were copied from, not edited.
- **Owner decisions applied:**
  - O1: quicknet for both seeds and seals, and an SBPF v2 build.
  - O2: no VRF.
  - M0 report §4.1–§4.5: pre-funding-safe initialisation, one unique BellAnchor, keeper-nonce SeedCaches, GatherClash, and ProveBadSeal with the FO check.
- **Build:**
  - `cargo-build-sbf 3.1.9`, platform-tools v1.52, `--arch v2`. The ELF `e_flags` is 2.
  - A v0 build is kept for comparison.
  - The builds are deterministic: two builds gave the same binary hashes (`results/binaries.txt`).
- **Runtime:** LiteSVM 0.16 with the mainnet feature set, the SIMD-0388 BLS12-381 syscalls, a 64-lock limit, and mainnet rent of 5,080 lamports per byte.
- **Network:** nothing was sent to devnet or mainnet, no validator was started, and no port was used.
- **Tags:** every number is [measured] unless it is tagged [estimate].

## 1. Results

| # | Item | Status | Numbers (SBPF v2; v0 in brackets) |
|---|---|---|---|
| 1 | Quicknet PostAnchor on v2 with hash-to-curve hints | **met** | PostAnchor **331,253–334,985 CU** over 32 real beacons [889k–900k] (m0c, with the archive key: 331,810–335,542 CU, 709 B). This includes the safe creation and storing the 48-B signature. Plain verification alone is 326,188–329,920 CU. The transaction is 676 B. A repeat post is a no-op at 1,504 CU. All results are within the design's 400k. |
| 2 | Account creation that pre-funding cannot block | **met** | For ArrivalSlot, SeedCache, SealVerdict and BellAnchor, plus ClashInputs and Citizen:<br>- After 650,240 lamports are sent to the address, the **old CreateAccount path fails** with `Custom(0)`, "account … already in use". This is the regression test.<br>- The **new path succeeds** on the same address. The payer tops up only rent − prefund.<br>- It also succeeds when the address is over-funded (10× rent).<br>- Cost of the new path: +1.6k CU for with-seed addresses (one more CPI). For the Citizen PDA it adds two CPIs; the bump search varies by wallet and dominates the Join spread. |
| 3 | One BellAnchor per (bell, region), with address checks | **met** | 18 refusal and consistency checks pass. Forged or alternative anchors and caches are refused with `Custom(9)` by PostAnchor, PostSeed, Reveal, GatherClash, ResolveClash and ProveBadSeal. Every seed round other than S(A) is refused with `Custom(7)`. |
| 4 | Nonce-keyed SeedCaches | **met** | Three keepers posted nonces 0, 7 and 255. All three caches hold the same seed, at 333,637 CU each (v0: 894,501). A repeat of a used nonce is a no-op at 2,189 CU. ResolveClash from each cache gave byte-identical provinces, and the digest equals the native kernel's. |
| 5 | GatherClash + ResolveClash worst case (24 ArrivalSlots, postures through GatherClash) | **met for bytes, locks and heap; CU is 2.3× the design's budget** | Hybrid ResolveClash (24 slots plus ClashInputs):<br>- worst found **632,641 CU**;<br>- **1,150 B**, **31 locks**, heap **25,872 B**, which is under 32 KiB, so no heap frame is needed;<br>- the digest matches the native kernel in every case.<br><br>Full-gather ResolveFromInputs: 540,892 CU, 358 B, 7 locks.<br>Gathers: at most 30,323 CU, 1,187 B, 32 locks. The hybrid needs 3 gathers; full gather needs 4.<br>v0 gives the same CU within 0.5%. |
| 6 | ProveBadSeal with the FO check | **met** | Up to **47,999 CU**, which includes reading the anchor, decompressing the signature and creating the verdict. A valid seal is refused (`Custom(14)`) at 44,376 CU. The transaction is 588 B. Every bad-seal case writes the correct code (8, 7, 7, 3). v0 is 47,860. |
| 7 | Join | **met** | **14,404–24,968 CU** (mean about 16k over 64 wallets per run). The spread comes from the Citizen PDA bump search. The transaction is 750 B with 15 locks. Join works when the Citizen address is pre-funded (safe init) and fails on the old path. Join is refused at capacity (`Custom(10)`). |
| + | Seed-round margin test | **met** | Unit test in the program crate: `seed_round_margin`, over 7,200 anchor times × every drand genesis offset, for periods of 3 s and 30 s. On chain:<br>- Reveal at A+599 lands; at A+600 and A+601 it is refused with `Custom(12)`.<br>- PostSeed with S−1 is refused (`Custom(7)`); S is accepted.<br>- The seed is published exactly 60 s after the close. |

The worst case in item 5 is the highest cost found in:
- 40 hand-built adversarial fills: dense, pile-up, random, spread12, spread12-neutral and wide;
- plus the top 12 by engagement count from 1,200 more fills screened with the native kernel.

It is not a proven upper bound.

## 2. Design used in the lab: the corrected accounts

- **Addresses.**
  - Every per-bell account uses a *with-seed* address of the Season PDA: `sha256(season ‖ seed ‖ program)`. Seeds:
    - `an‖hex(bell,region)` for the BellAnchor;
    - `sd‖hex(bell,region,nonce)` for SeedCaches;
    - `ar‖hex(P,Q,bell,f,i)` for ArrivalSlots;
    - `po‖hex(P,Q,bell,pos)` for PosturePDAs;
    - `ci‖hex(P,Q,bell)` for ClashInputs;
    - `sv‖hex(host,bell)` for SealVerdicts.
  - There is no bump, so each seed names exactly one address, and checking it costs one sha256.
  - Only the Citizen stays a PDA, `["cit", id, wallet]`.
- **Initialisation.**
  - Transfer (rent − lamports) if that is positive.
  - Then AllocateWithSeed, which allocates and assigns in one step, signed by the Season PDA. For a PDA, Allocate + Assign.
  - CreateAccount and CreateAccountWithSeed are kept only as "mode 1" for the regression test.
- **Absence.**
  - An account counts as *absent* when it is owned by the System program and has no data. Lamports are allowed.
  - Only the program can allocate these addresses, so a pre-funded address is still absent. This is tested in `t2b`: a pre-funded, never-revealed slot and posture address are both proven absent, and the resolve succeeds.
- **BellAnchor (112 B).**
  - Fields: season, bell, region, round T, A, slot, and the verified **48-B signature of round T(b)**.
  - The first post wins. Later posts are no-ops before any verification.
- **SeedCache (112 B).**
  - Fields: season, bell, region, nonce, round S, seed, **the anchor's key**, and A.
  - ResolveClash accepts any cache that is:
    - owned by the program;
    - at the canonical address for the nonce it records;
    - recording THE anchor's address;
    - holding a round equal to S(A);
    - with Clock ≥ A + 600.
- **Seed round.**
  - S(b, r) is the **first** round whose publication time is ≥ A + 600 + 60. This rounds up.
  - Reveal is refused once Clock ≥ A + 600.
  - Before THE anchor exists, the Reveal window is open (tested), **unless the region-day AnchorArchive tombstones the bell** (m0c, test `t9`).
- **Kernel.** The **real rules-v10 `frontier::clash::resolve_clash`** from `codex/frontier@cea89be`, copied into `rules/`, not M0's stand-in. On-chain results are compared with the same kernel run natively on the host, using the outcome digest.
- **ProveBadSeal.**
  - Reads the round signature from THE anchor of the seal's bell. Any region's anchor serves.
  - Checks `sha256(commit ‖ sha256(seal))` against the Transit record.
  - Runs S-TLOCK-V's opener with the FO check.
  - Writes a SealVerdict only for a bad seal. A valid seal gets `Custom(14)`.
- **Gathers commute.** Running the batches in reverse order and then all again (repeats are no-ops at 6.4k–10.1k CU) gives the native result (`t2b`).

## 3. Design impact (for revision 3)

1. **ResolveClash CU budget.**
   - The real v10 kernel alone costs **316k–510k CU** over the 40 fills, and more in the searched worst case. The whole instruction costs **367k–633k CU** over all 52 adversarial fills.
   - Rev2 used about 270k from M0's stand-in, and §8.9 used 210k. v0 and v2 are the same within 0.5%: v2 only helps the pairing-heavy crypto.
   - Budget about 650k per worst-case clash. The §8.9 capacity tables fall by about 2.3× from 270k, or 3× from 210k [estimate: linear in CU].
   - Kernel breakdown [measured, 40 fills, `t5b`]:
     - engagements 42%, about 1.8k CU per engagement (3 sha256 plus `resolve_engagement` each);
     - the **field-holding step 39%**: 116k–216k CU;
     - fair share 7%, validate/units 6%, retreat/caps 3%, apply 2%.
   - M1 kernel optimisation targets, both [estimate]:
     - the withdraw search in step 5, which scans O(withdrawing × 6 neighbours × units);
     - per-faction strength sums.
2. **Heap.** The peak is 25.9 KB of the default 32 KiB, a 21% margin. Any kernel growth needs RequestHeapFrame, which adds one instruction of about 5 B.
3. **Transaction shape.**
   - Hybrid ResolveClash (24 slots) is 1,150 B with 31 locks, leaving 82 B of headroom.
   - Full-gather ResolveFromInputs is 358 B with 7 locks, and costs about 21k CU less, because the slots are read in the gather. That comes at the price of one more gather transaction (about 30k CU).
   - Both fit. **Prefer full gather**: it leaves a smaller lock surface on the critical resolve and more byte headroom.
4. **Seed-round rule.**
   - State S(b, r) as the *first round published at or after* A + 600 + M. Do not use `round_at(A+600+M)`: that rounds down and can land up to period − 1 s early.
   - Keep M = 60.
5. **Absence rule: reverse the M0 wording.** M0 §4.1 said absence proofs "must reject a pre-funded system account". Doing so would let anyone block a province's ResolveClash for about 0.00065 SOL per slot address. The correct rule is: System-owned and empty is absent, whatever the lamports. The creation paths above make this safe.
6. **Anchor address.**
   - With a predictable with-seed address, "anchor absent = window still open" can be proven in Reveal.
   - The "unpredictable anchor address" defence (M0 §4.2 item 5), with `sha256(sig_T)` in the seed, breaks that proof. It would need an extra rule: before anchoring, Reveal is allowed only while Clock < time(T(b)) + 600. Not adopted here.
7. **ResolveClash does not read the anchor.**
   - It reads 28 accounts and trusts the A that PostSeed copied from THE anchor.
   - A cache whose A was moved together with its round can exist only through a program bug. In the lab such a cache is caught only by the clock check (`Custom(13)`).
   - For defence in depth, add THE anchor as a 29th key: +32 B, giving 1,182 B.
8. **Account sizes and rent at 5,080 lamports per byte.**

   | Account | Size | Rent (lamports) |
   |---|---|---|
   | BellAnchor | 112 B | 1,219,200 |
   | SeedCache | 112 B | 1,219,200 |
   | ArrivalSlot | 96 B | 1,137,920 |
   | SealVerdict | 96 B | 1,137,920 |
   | ClashInputs | 1,024 B | 5,852,160 |
   | Citizen | 320 B | 2,275,840 |

   PostSeed is 710 B when the keeper pays the fee, and 806 B with a separate keeper signer.
9. **Cost of the Reveal slot-creation part.** 6,356 CU (m0c, with the archive check: 6,603 CU). This covers reading the anchor, checking the address and the safe init. Commitment and path checks and quota ranking are not included.

## 4. Not covered

- The S-FEE redo and the anchor lock drill. They are a separate task.
- Not built:
  - the full Reveal (commitment, path, quota ranking and displacement);
  - CommitPosture and RevealPosture, as instructions (posture accounts were written directly in the tests);
  - SettleTransit and ArchiveAnchors.
- LiteSVM gives exact CU, sizes and locks. It has no cost tracker or fee market, so real-cluster confirmation is left to M4.
- The kernel is the `cea89be` copy. The M0 §4.8 bounds fixes may change its cost.

## 5. Reproduce

`./run.sh`. It is offline. It:
- builds the plain, trace and kernel-probe variants for v2 and v0;
- runs 3 program unit tests natively;
- runs 12 LiteSVM tests per architecture (11 before m0c's `t9`).

The log is in `results/run-log-full.txt`, and each test writes `results/<test>-<arch>.json`.

## 6. Links

- Lab: `scratchpad/frontier/m0b/spikes/SP-V2/`:
  - `program/src/{acct,beacon,clash,seal,join}.rs`
  - `host/tests/spv2.rs`
  - `results/`
  - `run.sh`
- Kernel copy: `rules/` (see `SOURCE.txt`). The probe-instrumented copy is `rules-probe/`, with `program-kprobe/`.
- Sources:
  - `scratchpad/frontier/m0/spikes/{S-BEACON,S-SIZE-JOIN,S-TLOCK}`
  - `scratchpad/frontier/m0/M0-REPORT.md`
