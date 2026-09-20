(function attachProofCore(globalScope) {
  "use strict";

  const SCHEMA_VERSION = "permutation-state.local-proof.v1";
  const BUILD_ID = "handoff-proof-2026-09-21";
  const DISCLOSURE = "LOCAL MULTI-CLIENT PROOF · NOT SOLANA · NOT LIVE AI";

  const PEOPLE = {
    mara: { id: "citizen-0214", name: "Mara Venn", civicRole: "ENVOY" },
    ivo: { id: "citizen-0708", name: "Ivo Sen", civicRole: "MAKER" },
    successor: { id: "citizen-1130", name: "Nia Quell", civicRole: "SUCCESSOR" },
    observer: { id: "observer", name: "Proof Observer", civicRole: "READ ONLY" }
  };

  const RESOLUTIONS = {
    service: {
      requires: "oath",
      ruleId: "EAST-SLUICE/R31-1",
      code: "R31-1",
      title: "Shared hands raised the gate",
      delta: { water: 24, cohesion: 4, timber: -6, prosperity: 5 },
      memory: "Tala remembers that Aster honored Mara's oath and worked beside the Riverkeepers.",
      mandates: [
        { id: "M-032", role: "MAKER", title: "Prepare Twelve Crates" },
        { id: "E-019", role: "ENVOY", title: "Write River Compact" },
        { id: "S-044", role: "SEEKER", title: "Inspect Flooded Archive" }
      ]
    },
    millrace: {
      requires: "oath",
      ruleId: "EAST-SLUICE/R31-2",
      code: "R31-2",
      title: "Old stone carries new water",
      delta: { water: 18, cohesion: -4, timber: -3, prosperity: 3 },
      memory: "Tala remembers the grain oath—and that Ivo bypassed the engineers it brought.",
      mandates: [
        { id: "E-020", role: "ENVOY", title: "Answer for the Bypass" },
        { id: "M-032", role: "MAKER", title: "Prepare Twelve Crates" },
        { id: "S-045", role: "SEEKER", title: "Survey Old Millrace" }
      ]
    },
    cut: {
      requires: "charter",
      ruleId: "EAST-SLUICE/R31-3",
      code: "R31-3",
      title: "Water returns; a fracture opens",
      delta: { water: 22, cohesion: -5, timber: -10, prosperity: 1 },
      memory: "Tala remembers that Aster invoked the Charter, then cut around the people who knew the river.",
      mandates: [
        { id: "E-021", role: "ENVOY", title: "Stop Riverkeeper Walkout" },
        { id: "W-014", role: "WARDEN", title: "Hold Eastern Cistern" },
        { id: "S-046", role: "SEEKER", title: "Record Charter Cut" }
      ]
    },
    reconcile: {
      requires: "charter",
      ruleId: "EAST-SLUICE/R31-4",
      code: "R31-4",
      title: "Terms reopen with the service stair",
      delta: { water: 18, cohesion: 8, timber: -6, prosperity: 4 },
      memory: "Tala remembers that a Maker repaired more than stone.",
      mandates: [
        { id: "M-033", role: "MAKER", title: "Prepare Eight Crates" },
        { id: "E-022", role: "ENVOY", title: "Ratify New Water Terms" },
        { id: "S-047", role: "SEEKER", title: "Map Reopened Stair" }
      ]
    }
  };

  function makeSeed(sessionId) {
    return {
      scenarioSeed: `ASTER:${sessionId}:EAST-SLUICE-031`,
      worldId: "aster",
      seasonId: "season-zero",
      epoch: 3,
      epochCount: 4,
      worksiteId: "east-sluice-031",
      publishedRuleset: "water-debt-ruleset-v0.3"
    };
  }

  function initialState() {
    return {
      worldId: "aster",
      seasonId: "season-zero",
      epoch: 3,
      epochCount: 4,
      seasonStatus: "active",
      seasonOutcome: "unresolved",
      purseStatus: "accumulating",
      settlementStatus: "not_started",
      claimAvailable: false,
      stage: "mara",
      resources: { water: 28, food: 46, cohesion: 54, timber: 18, prosperity: 38 },
      foodDebt: 0,
      talaTrust: 0,
      serviceStair: "closed",
      riverkeepers: "awaiting_envoy",
      memoryReceipt: null,
      npcMemory: "Tala has not yet met Aster's envoy.",
      maraChoice: null,
      ivoChoice: null,
      worksiteStatus: "active",
      worksiteResolution: null,
      nextMandates: [],
      acceptedMandate: null
    };
  }

  function clone(value) {
    return JSON.parse(JSON.stringify(value));
  }

  function canonicalize(value) {
    if (Array.isArray(value)) return value.map(canonicalize);
    if (value && typeof value === "object") {
      return Object.keys(value).sort().reduce((result, key) => {
        result[key] = canonicalize(value[key]);
        return result;
      }, {});
    }
    return value;
  }

  function canonicalJSON(value) {
    return JSON.stringify(canonicalize(value));
  }

  async function sha256Hex(value) {
    const cryptoApi = globalScope.crypto;
    if (!cryptoApi || !cryptoApi.subtle) throw new Error("Web Crypto SHA-256 is unavailable");
    const bytes = new TextEncoder().encode(typeof value === "string" ? value : canonicalJSON(value));
    const digest = await cryptoApi.subtle.digest("SHA-256", bytes);
    return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  }

  async function hashState(state) {
    return sha256Hex(canonicalJSON(state));
  }

  async function getGenesisHash(sessionId, seed) {
    return sha256Hex(canonicalJSON({ schemaVersion: SCHEMA_VERSION, sessionId, seed }));
  }

  function allowedIvoChoices(maraChoice) {
    if (maraChoice === "oath") return ["service", "millrace"];
    if (maraChoice === "charter") return ["cut", "reconcile"];
    return [];
  }

  function validateCommand(state, command) {
    if (!command || typeof command !== "object") throw new Error("Command is required");
    if (!Number.isInteger(command.expectedSeq)) throw new Error("expectedSeq is required");
    if (typeof command.expectedPrevHash !== "string") throw new Error("expectedPrevHash is required");

    if (command.type === "MARA_CHOICE") {
      if (command.actorRole !== "mara") throw new Error("Only Mara may make the envoy decision");
      if (state.stage !== "mara") throw new Error("Mara's decision has already been committed");
      if (!["oath", "charter"].includes(command.payload && command.payload.choice)) {
        throw new Error("Unknown Mara branch");
      }
      return;
    }

    if (command.type === "IVO_CHOICE") {
      if (command.actorRole !== "ivo") throw new Error("Only Ivo may resolve the East Sluice");
      if (state.stage !== "ivo") throw new Error("Ivo cannot act before Mara or after resolution");
      const choice = command.payload && command.payload.choice;
      if (!allowedIvoChoices(state.maraChoice).includes(choice)) {
        throw new Error(`Branch action ${choice || "(missing)"} is not unlocked by ${state.maraChoice || "no Mara choice"}`);
      }
      return;
    }

    if (command.type === "MANDATE_ACCEPTED") {
      if (command.actorRole !== "successor") throw new Error("Only the successor client may accept a generated Mandate");
      if (state.stage !== "successor") throw new Error("No generated Mandate is currently assignable");
      const mandateId = command.payload && command.payload.mandateId;
      if (!state.nextMandates.some((mandate) => mandate.id === mandateId && mandate.status === "queued")) {
        throw new Error(`Mandate ${mandateId || "(missing)"} is not in this branch's queue`);
      }
      return;
    }

    throw new Error(`Unsupported command type: ${command.type || "(missing)"}`);
  }

  function applyCommand(stateInput, command) {
    const state = clone(stateInput);
    validateCommand(state, command);

    if (command.type === "MARA_CHOICE") {
      const choice = command.payload.choice;
      state.maraChoice = choice;
      state.stage = "ivo";
      state.worksiteStatus = "access_resolved";
      if (choice === "oath") {
        state.resources.water = 36;
        state.resources.cohesion = 60;
        state.foodDebt = 12;
        state.talaTrust = 20;
        state.serviceStair = "open";
        state.riverkeepers = "present";
        state.memoryReceipt = "MR-031-OATH";
        state.npcMemory = "Mara promised twelve grain crates. Tala kept her engineers and opened the service stair.";
      } else {
        state.resources.water = 33;
        state.resources.cohesion = 46;
        state.foodDebt = 0;
        state.talaTrust = -20;
        state.serviceStair = "locked";
        state.riverkeepers = "withdrawn";
        state.memoryReceipt = "MR-031-CHARTER";
        state.npcMemory = "Mara invoked the Founding Charter. Tala complied, locked the service stair, and withdrew her engineers.";
      }
      return state;
    }

    if (command.type === "IVO_CHOICE") {
      const choice = command.payload.choice;
      const resolution = RESOLUTIONS[choice];
      Object.entries(resolution.delta).forEach(([key, delta]) => {
        state.resources[key] += delta;
      });
      if (choice === "service") state.talaTrust = 28;
      if (choice === "millrace") state.talaTrust = 8;
      if (choice === "cut") state.talaTrust = -35;
      if (choice === "reconcile") {
        state.talaTrust = 5;
        state.foodDebt = 8;
        state.serviceStair = "open";
        state.riverkeepers = "conditional_return";
      }
      state.ivoChoice = choice;
      state.worksiteResolution = resolution.code;
      state.worksiteStatus = choice === "cut" ? "stabilized_with_fracture" : "stabilized";
      state.npcMemory = resolution.memory;
      state.nextMandates = resolution.mandates.map((mandate) => ({ ...mandate, status: "queued" }));
      state.stage = "successor";
      return state;
    }

    const mandateId = command.payload.mandateId;
    state.nextMandates = state.nextMandates.map((mandate) => (
      mandate.id === mandateId ? { ...mandate, status: "assigned", assignedTo: PEOPLE.successor.id } : mandate
    ));
    state.acceptedMandate = mandateId;
    state.stage = "continuing";
    return state;
  }

  function eventHashPayload(event) {
    const { eventHash, ...payload } = event;
    return payload;
  }

  async function createEvent({ sessionId, seed, events, command, observedAt, clientId }) {
    const verification = await verifyLog({ sessionId, seed, events });
    if (!verification.valid) throw new Error(`Existing proof log is invalid: ${verification.errors.join("; ")}`);
    if (command.expectedSeq !== events.length) throw new Error("Stale command: sequence changed before commit");
    if (command.expectedPrevHash !== verification.headEventHash) throw new Error("Stale command: head hash changed before commit");

    const beforeState = verification.finalState;
    const beforeStateHash = verification.finalStateHash;
    const afterState = applyCommand(beforeState, command);
    const afterStateHash = await hashState(afterState);
    const event = {
      schemaVersion: SCHEMA_VERSION,
      sessionId,
      seq: events.length,
      type: command.type,
      actorRole: command.actorRole,
      actorId: PEOPLE[command.actorRole].id,
      payload: clone(command.payload),
      ruleId: command.ruleId,
      prevHash: verification.headEventHash,
      stateHashBefore: beforeStateHash,
      stateHashAfter: afterStateHash,
      observedAt,
      clientId,
      assertions: clone(command.assertions || [])
    };
    event.eventHash = await sha256Hex(canonicalJSON(eventHashPayload(event)));
    return { event, afterState };
  }

  async function verifyLog({ sessionId, seed, events }) {
    const errors = [];
    let state = initialState();
    let stateHash = await hashState(state);
    let headEventHash = await getGenesisHash(sessionId, seed);
    const validatedEvents = [];

    for (let index = 0; index < events.length; index += 1) {
      const event = events[index];
      try {
        if (event.schemaVersion !== SCHEMA_VERSION) throw new Error(`event ${index}: schema mismatch`);
        if (event.sessionId !== sessionId) throw new Error(`event ${index}: session mismatch`);
        if (event.seq !== index) throw new Error(`event ${index}: non-contiguous sequence`);
        if (event.prevHash !== headEventHash) throw new Error(`event ${index}: previous hash mismatch`);
        if (event.stateHashBefore !== stateHash) throw new Error(`event ${index}: before-state hash mismatch`);
        const expectedEventHash = await sha256Hex(canonicalJSON(eventHashPayload(event)));
        if (event.eventHash !== expectedEventHash) throw new Error(`event ${index}: event hash mismatch`);

        const command = {
          type: event.type,
          actorRole: event.actorRole,
          payload: event.payload,
          expectedSeq: index,
          expectedPrevHash: event.prevHash
        };
        const nextState = applyCommand(state, command);
        const nextStateHash = await hashState(nextState);
        if (event.stateHashAfter !== nextStateHash) throw new Error(`event ${index}: after-state hash mismatch`);
        state = nextState;
        stateHash = nextStateHash;
        headEventHash = event.eventHash;
        validatedEvents.push(event);
      } catch (error) {
        errors.push(error.message);
        break;
      }
    }

    return {
      valid: errors.length === 0 && validatedEvents.length === events.length,
      errors,
      finalState: state,
      finalStateHash: stateHash,
      headEventHash,
      genesisHash: await getGenesisHash(sessionId, seed),
      validatedEventCount: validatedEvents.length
    };
  }

  function flatten(value, prefix = "", target = {}) {
    if (Array.isArray(value)) {
      target[prefix] = value;
      return target;
    }
    if (value && typeof value === "object") {
      Object.keys(value).sort().forEach((key) => flatten(value[key], prefix ? `${prefix}.${key}` : key, target));
      return target;
    }
    target[prefix] = value;
    return target;
  }

  function stateDiff(before, after) {
    const left = flatten(before);
    const right = flatten(after);
    return Array.from(new Set([...Object.keys(left), ...Object.keys(right)])).sort().flatMap((path) => {
      if (canonicalJSON(left[path]) === canonicalJSON(right[path])) return [];
      return [{ path, before: left[path], after: right[path] }];
    });
  }

  function invariantResults(state, verification) {
    const results = [
      { id: "hash-chain", label: "Hash chain and event sequence", pass: verification.valid },
      { id: "season-active", label: "Season remains active", pass: state.seasonStatus === "active" },
      { id: "outcome-open", label: "Season outcome remains unresolved", pass: state.seasonOutcome === "unresolved" },
      { id: "purse-accumulating", label: "Purse still accumulating", pass: state.purseStatus === "accumulating" },
      { id: "settlement-closed", label: "Settlement has not started", pass: state.settlementStatus === "not_started" },
      { id: "claim-unavailable", label: "Claims unavailable", pass: state.claimAvailable === false }
    ];
    if (state.worksiteResolution) {
      results.push({ id: "three-mandates", label: "Resolution generated exactly 3 Mandates", pass: state.nextMandates.length === 3 });
      const expected = RESOLUTIONS[state.ivoChoice];
      results.push({
        id: "branch-resolution",
        label: "Resolution belongs to inherited branch",
        pass: Boolean(expected && expected.requires === state.maraChoice && expected.code === state.worksiteResolution)
      });
    }
    if (state.acceptedMandate) {
      results.push({
        id: "single-assignment",
        label: "Exactly one generated Mandate assigned",
        pass: state.nextMandates.filter((mandate) => mandate.status === "assigned").length === 1
      });
    }
    return results;
  }

  const api = {
    SCHEMA_VERSION,
    BUILD_ID,
    DISCLOSURE,
    PEOPLE,
    RESOLUTIONS,
    makeSeed,
    initialState,
    canonicalJSON,
    sha256Hex,
    hashState,
    getGenesisHash,
    allowedIvoChoices,
    validateCommand,
    applyCommand,
    createEvent,
    verifyLog,
    stateDiff,
    invariantResults,
    clone
  };

  globalScope.ProofCore = api;
  if (typeof module !== "undefined" && module.exports) module.exports = api;
})(typeof globalThis !== "undefined" ? globalThis : window);
