// Local Solana + MagicBlock ER stack for Wylls, on its own ports
// so it never touches another local validator:
//
//   node scripts/local-stack.mjs     base :18899 (ws :18900) · ER :17799 (ws :17800) · router :16699
//
// Wraps MagicBlock's `mb-stack` (base validator with the delegation/magic
// programs, then the ER, then the query router), adding permutation-chain
// and keeping the ledger and ER storage under this package's .local/.
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import path from 'node:path';
import { DEFAULTS, LOCAL_DIR as LOCAL, ROOT } from '../src/config.mjs';

const STACK = path.join(LOCAL, 'stack');
const PROGRAM_SO = path.resolve(ROOT, '../permutation-chain/target/deploy/permutation_chain.so');
const PROGRAM_ID = process.env.PS_PROGRAM_ID || DEFAULTS.programId;

if (!existsSync(PROGRAM_SO)) throw new Error(`build the program first: (cd ../permutation-chain && cargo build-sbf) — missing ${PROGRAM_SO}`);
// Only ever reset this package's own .local/stack directory.
rmSync(STACK, { recursive: true, force: true });
mkdirSync(STACK, { recursive: true });

const solanaBin = path.join(process.env.HOME, '.local/share/solana/install/active_release/bin');
// Extra programs the base validator needs but mb-stack does not bundle, as
// `<program id>.so` in .local/programs. The ER commits back to base through
// MagicBlock's committor program; without it every commit fails with
// ProgramAccountNotFound. Get it once (a public devnet program):
//   solana program dump -u devnet ComtrB2KEaWgXsW1dhr1xYL4Ht4Bjj3gXnnL6KMdABq .local/programs/ComtrB2KEaWgXsW1dhr1xYL4Ht4Bjj3gXnnL6KMdABq.so
const COMMITTOR = 'ComtrB2KEaWgXsW1dhr1xYL4Ht4Bjj3gXnnL6KMdABq';
const PROGRAMS = path.join(LOCAL, 'programs');
const extra = existsSync(PROGRAMS) ? readdirSync(PROGRAMS).filter(f => f.endsWith('.so')) : [];
if (!extra.includes(`${COMMITTOR}.so`)) console.warn(`warning: ${COMMITTOR}.so is missing from .local/programs: ER → base commits will fail (see the comment in this script)`);
const extraArgs = extra.flatMap(f => ['--bpf-program', f.slice(0, -3), path.join(PROGRAMS, f)]);

const child = spawn('mb-stack', ['--bpf-program', PROGRAM_ID, PROGRAM_SO, ...extraArgs, '--reset'], {
  cwd: STACK,
  stdio: 'inherit',
  env: {
    ...process.env,
    PATH: `${solanaBin}:${process.env.PATH}`,
    MB_STACK_BASE_PORT: process.env.PS_BASE_PORT || String(DEFAULTS.basePort),
    MB_STACK_ER_PORT: process.env.PS_ER_PORT || String(DEFAULTS.erPort),
    MB_STACK_PUBLIC_PORT: process.env.PS_ROUTER_PORT || String(DEFAULTS.routerPort),
    // The ER wrapper defaults to RUST_LOG=quiet; mb-stack forwards only lines
    // that mention error/failed, so warnings are enough to see failed commits.
    RUST_LOG: process.env.RUST_LOG || 'warn',
  },
});
for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => child.kill(sig));
child.on('exit', code => process.exit(code ?? 0));
