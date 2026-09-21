import assert from "node:assert/strict";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const Core = require("../../permutation-state-prototype/proof/core.js");
const baseUrl = (process.env.PERMSTATE_GATEWAY_URL || "http://127.0.0.1:4173").replace(/\/$/, "");
const sessionId = `e2e-${Date.now().toString(36)}`;
const clientId = "local-e2e-runner";

async function request(path, options = {}) {
  const response = await fetch(`${baseUrl}${path}`, {
    ...options,
    headers: { "Content-Type": "application/json", ...(options.headers || {}) },
  });
  const body = await response.json();
  if (!response.ok) throw new Error(body.error || `${response.status} ${response.statusText}`);
  return body;
}

async function append(type, actorRole, payload, ruleId, assertions) {
  const session = await request(`/api/magicblock/session?session=${encodeURIComponent(sessionId)}`);
  const { store } = session;
  const verified = await Core.verifyLog({ sessionId, seed: store.seed, events: store.events });
  assert.equal(verified.valid, true);
  const { event } = await Core.createEvent({
    sessionId,
    seed: store.seed,
    events: store.events,
    observedAt: new Date().toISOString(),
    clientId,
    command: {
      type,
      actorRole,
      payload,
      ruleId,
      assertions,
      expectedSeq: store.events.length,
      expectedPrevHash: verified.headEventHash,
    },
  });
  return request("/api/magicblock/action", {
    method: "POST",
    body: JSON.stringify({ session: sessionId, event }),
  });
}

await request("/api/magicblock/health");
const bootstrap = await request("/api/magicblock/bootstrap", {
  method: "POST",
  body: JSON.stringify({ session: sessionId }),
});
assert.equal(bootstrap.initialized, true);
assert.equal(bootstrap.delegated, true);
assert.equal(bootstrap.state.seq, 0);

const mara = await append(
  "MARA_CHOICE",
  "mara",
  { choice: "oath" },
  "RIVER-GUILD/GRAIN-OATH",
  ["actor=mara", "worksite=active", "branch=oath"],
);
assert.equal(mara.state.seq, 1);
assert.equal(mara.state.branch, 1);
assert.equal(mara.network.lastReceipt.er.layer, "ephemeral-rollup");

const ivo = await append(
  "IVO_CHOICE",
  "ivo",
  { choice: "service" },
  Core.RESOLUTIONS.service.ruleId,
  ["actor=ivo", "inherited=oath", "allowed=service|millrace"],
);
assert.equal(ivo.state.seq, 2);
assert.equal(ivo.state.resolution, 1);
assert.equal(ivo.network.lastReceipt.checkpoint.settlement, "checkpoint-read-back-verified");
assert.equal(ivo.network.lastReceipt.checkpoint.stateRoot, ivo.network.lastReceipt.er.stateRoot);

const successor = await append(
  "MANDATE_ACCEPTED",
  "successor",
  { mandateId: "M-032" },
  "MANDATE/ASSIGN-V1",
  ["actor=successor", "mandate=generated", "status=queued"],
);
assert.equal(successor.state.seq, 3);
assert.equal(successor.state.acceptedMandate, 1032);
assert.equal(successor.store.events.length, 3);
assert.equal(successor.state.stateRootMatches, true);

console.log(JSON.stringify({
  ok: true,
  sessionId,
  programId: successor.descriptor.programId,
  worksitePda: successor.descriptor.worksitePda,
  erSignature: successor.network.lastReceipt.er.signature,
  checkpointSignature: ivo.network.lastReceipt.checkpoint.signature,
  finalSequence: successor.state.seq,
  acceptedMandate: successor.state.acceptedMandate,
}, null, 2));
