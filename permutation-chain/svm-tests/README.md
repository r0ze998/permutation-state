# permutation-chain program tests (LiteSVM)

These tests run the SBF build of the program
(`permutation-chain/target/deploy/permutation_chain.so`) in LiteSVM 0.16
(the Agave 4.2.2 runtime). They use real SPL Token transfers, a real
clock, the real compute meter and the real heap frame. The account
layouts come from the program crate itself, built without its `program`
feature, so there are no hand-kept mirrors. The play server's bots play
whole seasons in-process.

## Running

```sh
permutation-chain/svm-tests/run.sh                    # build the program, run everything
permutation-chain/svm-tests/run.sh --test settlement  # any cargo test arguments
permutation-chain/svm-tests/run.sh -- --nocapture     # print the CU/heap numbers
```

**Always use `run.sh`.** It builds the program first with
`scripts/build-program.sh`, the reproducible build with the default
features. A plain `cargo test` here tests whatever `.so` is already in
`target/deploy`. The harness prints that binary's path, sha256 and age once
per test process. It warns when the binary is older than the newest file
under `permutation-chain/src` or `permutation-rules/src`.

This crate has its own `[workspace]`, `Cargo.lock` and toolchain (1.95.0;
LiteSVM does not build on 1.89). It is excluded from the root workspace and
builds into its own `target/`.

| Variable | Effect |
|---|---|
| `PERMUTATION_CHAIN_SO` | the binary to test (`run.sh` sets it to the fresh build) |
| `RELEASE_CHECK=1` | the coverage guard also fails while any test waits for a fix (PENDING) or any instruction is `Cover::Pending`. CI sets it on tags and `release/*` |
| `OLD_REF` / `OLD_SO` | `chain::tests::swap_program_mid_season` starts the season on that older program (`run.sh` builds `OLD_REF` into `target/old`). Without it, the binary under test is swapped for itself |
| `DLP_SO` | `Chain::with_real_dlp` loads a real delegation program instead of the stand-in (opt-in tests) |
| `SVM_HEAVY=1` | with `--ignored`, `budget::finish_after_played_season_at_the_cap` (a 256-member played season, minutes). At this commit it stops at OpenGovernment: 256 bot members voting in the first election exceed 1.4M CU (the election and member-cap work, WP06/WP07) |
| `PLAYED_TICKS=n` | `lifecycle::bots_play_a_blitz_season_on_the_sbf_build` stops after n ticks |
| `NEED_EVERY=k` | `budget::played_blitz_15` measures every k-th tick instead of 44, 89, 134 and 179 |
| `MEMBERS=n` | `budget::idle_at_member_cap` with n members instead of `MAX_MEMBERS` |

## Layout

| Path | What |
|---|---|
| `src/chain.rs` | `Chain`: the SVM, `send` with the client's budget profile, `send_as` (no signatures), accounts, the clock, `fork`, `upgrade`, `assert_err` / `assert_token_err` / `assert_program_err` |
| `src/spl.rs` | hand-packed SPL Token mints and token accounts, balances |
| `src/season.rs` | `SeasonFx`: a season built through the program (`create`, `create_ai`, `register`, `register_ai`, …) |
| `src/ix/*.rs` | instruction builders, one file per program area; account order equals `permutation-gateway/client/src/chain.mjs` (each cites its line) |
| `src/drive/*.rs` | genesis, seating, ticks (`play_tick`, `close`, `log_input`), `fast_forward_to_end`, `settle` |
| `src/bots.rs` | `BotSeason`: the play server's bots (seeded ledger) through the program, chain = native every tick |
| `src/magicblock.rs` | recording stand-ins for the Magic and delegation programs, intent and delegate-args decoding, the undelegate callback |
| `src/records.rs` | `PS_*` log records |
| `src/budget.rs` | `need` (CU and bisected heap), the ceilings, `crank_stops()`, `resolve_parts` |
| `src/cover/*.rs` | the coverage tables and the PENDING ledgers, one file per area |
| `tests/*.rs` | `registration`, `genesis`, `play`, `play_gate`, `delegation`, `roster`, `settlement`, `lifecycle`, `invariants`, `budget`, `coverage` |

## Budget profiles

Every test sends with the budget the real client uses (`chain::client_budget`
maps each tag to the profile `chain.mjs` gives it):

| Profile | Compute-budget instructions | Instructions | Ceilings (`budget.rs`) |
|---|---|---|---|
| Heavy | 1.4M CU, 256 KiB heap | StartSeason, GenesisStep, SeatMembers, OpenGovernment, CloseCommits, LogTickInput, ResolveTick, FinishSeason | 1.2M CU, 224 KiB |
| Medium | 128 KiB heap | CommitOrders, RevealOrders, SubmitGov | 170k CU, 112 KiB |
| Light | none (200k CU, 32 KiB heap) | everything else | 170k CU, lands in the default frame |

The margins are about 14% CU and 12.5% heap under what the client requests,
for every profile. `Chain::need` finds the heap an instruction needs by
bisecting the heap frame in 1 KiB steps on forks. This works because the
program's bump allocator faults past the frame it was given. A Light
instruction cannot be bisected below 32 KiB, so the rule for it is
structural: an instruction whose heap grows with the season must not be
Light in the client.

ResolveTick is always sent and measured in the crank's parts. From phase 0
the parts are the consecutive split points of `STOPS` in
`permutation-gateway/src/ticks.mjs`. `crank_stops()` reads that literal
line at compile time, so a change to the crank's split points changes the
budget tests with it. Keep `STOPS` and `LAST_PHASE` literal.

## Rules

- **Signer checks run with sigverify on and real keypairs.**
  - `Chain::new_opts(false)` and `send_as` are only for the bots, whose
    session keys are `local_key` hashes without private keys, and for the
    DLP callback, whose buffer the DLP signs by CPI.
  - `coverage::signer_tests_use_sigverify` checks this for
    `registration.rs`, `settlement.rs` and `roster.rs`.
- **Every instruction and every error code has a test.**
  - `src/cover` lists, per instruction, the tests and the outcomes each
    asserts. `covered_by` is an exhaustive `match`, so a new instruction
    does not compile until it is listed.
  - `coverage.rs` checks each listed test exists, is not ignored, and
    that its body (comments stripped) contains `E::X`, `assert_token_err(`,
    the error name, or the builder and `.expect(`/`.unwrap()`.
  - Every `ChainError` must be asserted by some listed test, unless it is
    on `EXEMPT` (empty).
- **Ignored tests are on a ledger.**
  - A test written for behaviour a later work package fixes is
    `#[ignore = "until WPxx: …"]` and has a line in its area's `PENDING`
    (`src/cover/<area>.rs`). The fix removes both in the same change.
  - An opt-in test names its environment variable in its reason and is
    on `OPT_IN`.
  - `coverage::ignores_are_accounted_for` prints the ledger, grouped by
    work package, on every run.
- **The play server builds the chain crate without `program`.**
  - Code the server or the verifier calls lives in the always-compiled
    modules (`permutation-chain/src/lib.rs`).
  - `coverage::server_never_enables_program` names the rule; the builds
    enforce it.
- **The binary is overflow-checked.** `chain::tests::the_program_is_built_with_overflow_checks`
  fails on a `.so` built without overflow checks for the program's crate
  (root `Cargo.toml`); `scripts/build-program.sh` refuses such a build too.
- **Fixtures are built in-process.** Seasons come from `SeasonFx` and
  `BotSeason`; bot seasons use `Ledger::seeded`. No test mirrors a layout
  or adds a second LiteSVM crate.

## What this suite does not prove

LiteSVM stands in for both the base layer and the Ephemeral Rollup, and the
delegation and Magic programs are recording stand-ins. So these tests do
not show:

- that the delegation program and the committor accept and finalize the
  intents the program schedules;
- the ER's clock, ordering and auto-commit;
- CU differences between Agave versions (hence the margins).

The local-stack and devnet end-to-end runs (`permutation-gateway/scripts/`)
cover those.

## PENDING at this commit

| Test | Waits for |
|---|---|
| `play_gate::seating_attacks_fail` | WP01 |
| `delegation::undelegation_is_the_cranks_until_the_window_closes`, `delegation::undelegation_intents_have_the_crank_shape` | WP02 |
| `budget::light_intents_at_every_target` (Commit over 26 accounts 174–177k CU, CommitPart at every target 188k CU under Light) | WP02 |
| `budget::inbox_full_quota_log_input` (LogTickInput#0 at 232 KiB with full inboxes) | WP03/WP07 |
| `budget::only_buildable_seasons_are_creatable` (the Season preset is creatable but its first GenesisStep runs out of 1.4M CU) | WP13 |
| `roster::reveal_roster_needs_the_operator_and_the_end`, `roster::self_reveal_cannot_break_the_roster` | WP09 |
| `registration::register_needs_the_session_signature` | WP17 |
