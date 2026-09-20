(() => {
  "use strict";

  const Core = window.ProofCore;
  const params = new URLSearchParams(window.location.search);
  const validRoles = ["mara", "ivo", "successor", "observer"];
  const rawRole = params.get("role") || "observer";
  const role = validRoles.includes(rawRole) ? rawRole : "observer";
  const sessionId = sanitizeSession(params.get("session") || "aster-demo");
  const storageKey = `permutation-state-proof:${sessionId}`;
  const channelName = `permutation-state-proof:${sessionId}`;
  const storageOnlyTransport = params.get("transport") === "storage";
  const clientKey = `permutation-state-proof-client:${sessionId}:${role}`;
  const clientId = sessionStorage.getItem(clientKey) || crypto.randomUUID();
  sessionStorage.setItem(clientKey, clientId);

  const channel = !storageOnlyTransport && "BroadcastChannel" in window ? new BroadcastChannel(channelName) : null;
  const presence = new Map();
  let store = loadOrCreateStore();
  let verification = null;
  let renderRevision = 0;

  const elements = {
    app: document.getElementById("proof-app"),
    sessionLabel: document.getElementById("session-label"),
    roleNav: document.getElementById("role-nav"),
    syncLabel: document.getElementById("sync-label"),
    metricList: document.getElementById("metric-list"),
    worldStatus: document.getElementById("world-status"),
    stateHash: document.getElementById("state-hash"),
    headHash: document.getElementById("head-hash"),
    actorAvatar: document.getElementById("actor-avatar"),
    actorRole: document.getElementById("actor-role"),
    actorName: document.getElementById("actor-name"),
    actorMode: document.getElementById("actor-mode"),
    actorWorkspace: document.getElementById("actor-workspace"),
    eventCount: document.getElementById("event-count"),
    agreementLabel: document.getElementById("agreement-label"),
    clientList: document.getElementById("client-list"),
    invariantScore: document.getElementById("invariant-score"),
    invariantList: document.getElementById("invariant-list"),
    eventList: document.getElementById("event-list"),
    observerActions: document.getElementById("observer-actions"),
    toast: document.getElementById("toast")
  };

  function sanitizeSession(value) {
    const cleaned = String(value).toLowerCase().replace(/[^a-z0-9-]/g, "-").replace(/-+/g, "-").replace(/^-|-$/g, "");
    return (cleaned || "aster-demo").slice(0, 40);
  }

  function newStore() {
    return {
      schemaVersion: Core.SCHEMA_VERSION,
      buildId: Core.BUILD_ID,
      sessionId,
      localProofDisclosure: Core.DISCLOSURE,
      seed: Core.makeSeed(sessionId),
      createdAt: new Date().toISOString(),
      events: []
    };
  }

  function loadOrCreateStore() {
    const raw = localStorage.getItem(storageKey);
    if (!raw) {
      const created = newStore();
      localStorage.setItem(storageKey, JSON.stringify(created));
      return created;
    }
    try {
      return JSON.parse(raw);
    } catch {
      return { ...newStore(), events: [], integrityError: "Stored session is not valid JSON" };
    }
  }

  function reloadStore() {
    try {
      const parsed = JSON.parse(localStorage.getItem(storageKey));
      if (parsed) store = parsed;
    } catch {
      store = { ...newStore(), integrityError: "Stored session is not valid JSON" };
    }
    return store;
  }

  function saveStore(nextStore) {
    localStorage.setItem(storageKey, JSON.stringify(nextStore));
    store = nextStore;
  }

  function currentUrl(nextRole) {
    const url = new URL(window.location.href);
    url.search = "";
    url.searchParams.set("proof", "1");
    url.searchParams.set("session", sessionId);
    url.searchParams.set("role", nextRole);
    if (storageOnlyTransport) url.searchParams.set("transport", "storage");
    return url.toString();
  }

  function escapeHtml(value) {
    return String(value)
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#039;");
  }

  function shortHash(value) {
    if (!value) return "—";
    return `${value.slice(0, 8)}…${value.slice(-6)}`;
  }

  function pretty(value) {
    if (value === null || value === undefined || value === "") return "—";
    if (typeof value === "boolean") return value ? "YES" : "NO";
    if (Array.isArray(value)) return `${value.length} items`;
    return String(value).replaceAll("_", " ").toUpperCase();
  }

  function setRoleNav() {
    const labels = { mara: "MARA · A", ivo: "IVO · B", successor: "NEXT · C", observer: "OBSERVER" };
    elements.roleNav.innerHTML = validRoles.map((item) => (
      `<a class="${item === role ? "active" : ""}" data-role-link="${item}" href="${escapeHtml(currentUrl(item))}" target="${item === role ? "_self" : "_blank"}">${labels[item]}</a>`
    )).join("");
  }

  function checkStoreEnvelope(candidate) {
    const errors = [];
    if (candidate.integrityError) errors.push(candidate.integrityError);
    if (candidate.schemaVersion !== Core.SCHEMA_VERSION) errors.push("Session schema does not match this build");
    if (candidate.buildId !== Core.BUILD_ID) errors.push("Session build does not match this proof harness");
    if (candidate.sessionId !== sessionId) errors.push("Session identifier mismatch");
    if (!candidate.seed || !Array.isArray(candidate.events)) errors.push("Session envelope is incomplete");
    return errors;
  }

  async function refresh(source = "local") {
    const revision = ++renderRevision;
    reloadStore();
    const envelopeErrors = checkStoreEnvelope(store);
    let nextVerification;
    if (envelopeErrors.length) {
      const initial = Core.initialState();
      nextVerification = {
        valid: false,
        errors: envelopeErrors,
        finalState: initial,
        finalStateHash: await Core.hashState(initial),
        headEventHash: store.seed ? await Core.getGenesisHash(sessionId, store.seed) : "unavailable",
        genesisHash: store.seed ? await Core.getGenesisHash(sessionId, store.seed) : "unavailable",
        validatedEventCount: 0
      };
    } else {
      nextVerification = await Core.verifyLog({ sessionId, seed: store.seed, events: store.events });
    }
    if (revision !== renderRevision) return;
    verification = nextVerification;
    render(source);
    announcePresence();
  }

  async function withSessionLock(callback) {
    if (navigator.locks && navigator.locks.request) {
      return navigator.locks.request(`permutation-state-proof-lock:${sessionId}`, { mode: "exclusive" }, callback);
    }
    return callback();
  }

  async function commitAction(type, payload) {
    if (role === "observer") throw new Error("Observer is read-only");
    return withSessionLock(async () => {
      reloadStore();
      const envelopeErrors = checkStoreEnvelope(store);
      if (envelopeErrors.length) throw new Error(`Integrity halt: ${envelopeErrors.join("; ")}`);
      const current = await Core.verifyLog({ sessionId, seed: store.seed, events: store.events });
      if (!current.valid) throw new Error(`Integrity halt: ${current.errors.join("; ")}`);

      const ruleId = type === "MARA_CHOICE"
        ? (payload.choice === "oath" ? "RIVER-GUILD/GRAIN-OATH" : "RIVER-GUILD/FOUNDING-CHARTER")
        : type === "IVO_CHOICE"
          ? Core.RESOLUTIONS[payload.choice].ruleId
          : "MANDATE/ASSIGN-V1";
      const assertions = type === "MARA_CHOICE"
        ? ["actor=mara", "worksite=active", `branch=${payload.choice}`]
        : type === "IVO_CHOICE"
          ? ["actor=ivo", `inherited=${current.finalState.maraChoice}`, `allowed=${Core.allowedIvoChoices(current.finalState.maraChoice).join("|")}`]
          : ["actor=successor", "mandate=generated", "status=queued"];

      const command = {
        type,
        actorRole: role,
        payload,
        ruleId,
        assertions,
        expectedSeq: store.events.length,
        expectedPrevHash: current.headEventHash
      };
      const { event } = await Core.createEvent({
        sessionId,
        seed: store.seed,
        events: store.events,
        command,
        observedAt: new Date().toISOString(),
        clientId
      });
      const nextStore = { ...store, events: [...store.events, event], updatedAt: new Date().toISOString() };
      saveStore(nextStore);
      if (channel) channel.postMessage({ type: "SESSION_UPDATED", sourceClientId: clientId, eventHash: event.eventHash });
      await refresh("commit");
      showToast(`Accepted ${event.type} · ${shortHash(event.eventHash)}`, "success");
      return event;
    }).catch((error) => {
      showToast(error.message, "error");
      throw error;
    });
  }

  function render(source) {
    const state = verification.finalState;
    elements.app.dataset.role = role;
    elements.app.dataset.integrity = verification.valid ? "valid" : "invalid";
    elements.sessionLabel.textContent = sessionId;
    elements.syncLabel.textContent = verification.valid ? (source === "storage" || source === "broadcast" ? "SYNCED NOW" : "REPLAY VERIFIED") : "INTEGRITY HALT";
    elements.syncLabel.classList.toggle("bad", !verification.valid);
    renderMetrics(state);
    renderWorldStatus(state);
    elements.stateHash.textContent = shortHash(verification.finalStateHash);
    elements.stateHash.title = verification.finalStateHash;
    elements.headHash.textContent = shortHash(verification.headEventHash);
    elements.headHash.title = verification.headEventHash;
    renderCausalStrip(state);
    renderActor(state);
    renderInvariants(state);
    renderEvents();
    renderPresence();
    elements.observerActions.hidden = role !== "observer";
  }

  function renderMetrics(state) {
    const baseline = Core.initialState().resources;
    const labels = { water: "WATER", food: "FOOD", cohesion: "COHESION", timber: "TIMBER", prosperity: "PROSPERITY" };
    elements.metricList.innerHTML = Object.entries(state.resources).map(([key, value]) => {
      const delta = value - baseline[key];
      const deltaText = delta === 0 ? "±0" : `${delta > 0 ? "+" : ""}${delta}`;
      return `<div class="metric"><span>${labels[key]}</span><div><i style="width:${Math.max(4, Math.min(100, value))}%"></i></div><strong>${value}</strong><em class="${delta < 0 ? "negative" : delta > 0 ? "positive" : ""}">${deltaText}</em></div>`;
    }).join("");
  }

  function renderWorldStatus(state) {
    elements.worldStatus.innerHTML = `
      <div><span>SEASON</span><strong>${pretty(state.seasonStatus)}</strong></div>
      <div><span>WORKSITE 31</span><strong>${pretty(state.worksiteResolution || state.worksiteStatus)}</strong></div>
      <div><span>FOOD DEBT</span><strong>${state.foodDebt}</strong></div>
      <div><span>SETTLEMENT</span><strong>${pretty(state.settlementStatus)}</strong></div>
      <div><span>CLAIM</span><strong>${state.claimAvailable ? "AVAILABLE" : "UNAVAILABLE"}</strong></div>
      <div><span>STAGE</span><strong>${pretty(state.stage)}</strong></div>`;
  }

  function renderCausalStrip(state) {
    const progress = { mara: 0, ivo: 2, successor: 3, continuing: 4 }[state.stage] || 0;
    const steps = document.querySelectorAll("[data-causal-step]");
    steps.forEach((step, index) => {
      step.classList.toggle("done", index < progress);
      step.classList.toggle("active", index === progress || (state.stage === "ivo" && index === 1));
    });
  }

  function renderActor(state) {
    const person = Core.PEOPLE[role];
    elements.actorAvatar.textContent = role === "observer" ? "◎" : person.name.split(" ").map((part) => part[0]).join("");
    elements.actorRole.textContent = `${person.civicRole} · ${person.id.toUpperCase()}`;
    elements.actorName.textContent = person.name;
    elements.actorMode.textContent = role === "observer" ? "READ ONLY" : "ROLE LOCKED BY URL";

    if (!verification.valid) {
      elements.actorWorkspace.innerHTML = `<div class="integrity-halt"><span>FAIL CLOSED</span><h2>Proof log integrity failed.</h2><p>No client may append a new event until this local session is reset.</p><code>${escapeHtml(verification.errors.join(" · "))}</code></div>`;
      return;
    }
    if (role === "mara") renderMara(state);
    if (role === "ivo") renderIvo(state);
    if (role === "successor") renderSuccessor(state);
    if (role === "observer") renderObserver(state);
  }

  function choiceCard({ id, eyebrow, title, body, facts, disabled = false }) {
    return `<button type="button" class="choice-card" data-choice="${id}" ${disabled ? "disabled" : ""}>
      <span>${eyebrow}</span><h3>${title}</h3><p>${body}</p>
      <footer>${facts.map((fact) => `<em>${fact}</em>`).join("")}</footer>
    </button>`;
  }

  function renderMara(state) {
    if (!state.maraChoice) {
      elements.actorWorkspace.innerHTML = `
        <div class="scene-kicker"><span>PLAYER A · EAST SLUICE ACCESS</span><em>EVENT 0 EXPECTED</em></div>
        <h2 class="scene-title">Tala controls the only safe route below the river.</h2>
        <p class="scene-copy">Choose how Aster gets access. Your accepted event will leave this tab and change Ivo's legal action set in another client.</p>
        <blockquote>“Bring me a promise the city must remember—or bring me the old law.” <cite>— Tala, River Guild</cite></blockquote>
        <div class="choice-grid">
          ${choiceCard({ id: "oath", eyebrow: "RELATIONAL ROUTE", title: "Bind the Grain Oath", body: "Promise twelve crates after harvest. Tala keeps her engineers and opens the service stair.", facts: ["FOOD DEBT +12", "TRUST +20", "STAIR OPEN"] })}
          ${choiceCard({ id: "charter", eyebrow: "LEGAL ROUTE", title: "Invoke the Founding Charter", body: "Compel access without a grain promise. Tala withdraws her engineers and seals the stair.", facts: ["NO DEBT", "TRUST −20", "STAIR LOCKED"] })}
        </div>`;
      elements.actorWorkspace.querySelectorAll("[data-choice]").forEach((button) => {
        button.addEventListener("click", () => commitAction("MARA_CHOICE", { choice: button.dataset.choice }));
      });
      return;
    }
    const oath = state.maraChoice === "oath";
    elements.actorWorkspace.innerHTML = `
      <div class="accepted-mark">EVENT 0 ACCEPTED · PROPAGATED TO OTHER CLIENTS</div>
      <div class="receipt-card">
        <span>MEMORY RECEIPT</span><strong>${state.memoryReceipt}</strong>
        <h2>${oath ? "The city now owes what Mara promised." : "The city got access and lost cooperation."}</h2>
        <p>${escapeHtml(state.npcMemory)}</p>
        <div class="receipt-facts"><em>BRANCH ${state.maraChoice.toUpperCase()}</em><em>SERVICE STAIR ${state.serviceStair.toUpperCase()}</em><em>RIVERKEEPERS ${pretty(state.riverkeepers)}</em></div>
      </div>
      <p class="waiting-copy">This client is finished. Ivo's separate URL now contains only the actions unlocked by this history.</p>
      <a class="primary link-button" href="${escapeHtml(currentUrl("ivo"))}" target="_blank">OPEN IVO'S SEPARATE CLIENT →</a>`;
  }

  function ivoRibbon(state) {
    if (state.maraChoice === "oath") {
      return `<div class="causal-ribbon"><strong>MARA PROMISED 12 FOOD</strong><i>→</i><strong>TALA KEPT HER ENGINEERS</strong><i>→</i><strong>SERVICE STAIR UNLOCKED FOR YOU</strong><small>Counterfactual: without this oath, Service Stair locked · Riverkeepers absent</small></div>`;
    }
    return `<div class="causal-ribbon fracture"><strong>MARA INVOKED THE CHARTER</strong><i>→</i><strong>TALA WITHDREW ENGINEERS</strong><i>→</i><strong>CUT / RECONCILE ONLY</strong><small>Counterfactual: with the oath, Service Stair repair · Riverkeepers present</small></div>`;
  }

  function renderIvo(state) {
    if (!state.maraChoice) {
      elements.actorWorkspace.innerHTML = `
        <div class="waiting-state"><span>WAITING FOR EVENT 0</span><h2>Ivo cannot act before Mara.</h2><p>This is enforced by the reducer, not hidden by the interface. Keep this tab open; the accepted event will appear automatically.</p><div class="pulse-line"></div></div>`;
      return;
    }
    if (!state.ivoChoice) {
      const oath = state.maraChoice === "oath";
      elements.actorWorkspace.innerHTML = `
        ${ivoRibbon(state)}
        <div class="scene-kicker"><span>PLAYER B · INHERITED WORLD</span><em>${state.memoryReceipt}</em></div>
        <h2 class="scene-title">Your action space was written by another citizen.</h2>
        <p class="scene-copy">Tala's authored line expresses the memory. The two legal actions and their resource effects come from the fixed resolver.</p>
        <blockquote>“${oath ? "Mara spoke for Aster. Twelve crates after harvest. I opened the service stair for you, Maker." : "Your envoy brought law, not trust. The city may have its water. It will not have my help."}” <cite>— Tala, inherited memory</cite></blockquote>
        <div class="choice-grid">
          ${oath
            ? choiceCard({ id: "service", eyebrow: "UNLOCKED BY MARA'S OATH", title: "Repair through Service Stair", body: "Work beside the Riverkeepers and honor the debt that opened this path.", facts: ["WATER +24", "TIMBER −6", "R31-1"] })
              + choiceCard({ id: "millrace", eyebrow: "AVAILABLE FALLBACK", title: "Reopen the Old Millrace", body: "Bypass Tala's engineers without erasing Mara's promise.", facts: ["WATER +18", "TIMBER −3", "R31-2"] })
            : choiceCard({ id: "cut", eyebrow: "CHARTER-ONLY ROUTE", title: "Cut through the Sealed Millrace", body: "Restore water without Riverkeeper help and deepen the fracture.", facts: ["WATER +22", "COHESION −5", "R31-3"] })
              + choiceCard({ id: "reconcile", eyebrow: "CHARTER-ONLY ROUTE", title: "Return and Reconcile", body: "Negotiate a smaller obligation and reopen the service stair.", facts: ["FOOD DEBT +8", "TRUST +25", "R31-4"] })}
        </div>`;
      elements.actorWorkspace.querySelectorAll("[data-choice]").forEach((button) => {
        button.addEventListener("click", () => commitAction("IVO_CHOICE", { choice: button.dataset.choice }));
      });
      return;
    }

    const resolution = Core.RESOLUTIONS[state.ivoChoice];
    elements.actorWorkspace.innerHTML = `
      ${ivoRibbon(state)}
      <div class="accepted-mark">EVENT 1 ACCEPTED · ${resolution.code} · SEASON STILL ACTIVE</div>
      <div class="resolution-card">
        <span>LOCAL WORKSITE RESOLUTION</span><strong>${resolution.code}</strong>
        <h2>${resolution.title}.</h2><p>${escapeHtml(resolution.memory)}</p>
        <div class="resolution-rule"><em>SEASON ACTIVE</em><em>PURSE ACCUMULATING</em><em>CLAIM UNAVAILABLE</em></div>
      </div>
      <div class="mandate-preview"><span>THREE NEW JOBS NOW EXIST FOR OTHER CITIZENS</span>${state.nextMandates.map((mandate) => `<b>${mandate.id} · ${mandate.role} · ${escapeHtml(mandate.title)}</b>`).join("")}</div>
      <a class="primary link-button" href="${escapeHtml(currentUrl("successor"))}" target="_blank">OPEN THE NEXT CITIZEN'S CLIENT →</a>`;
  }

  function renderSuccessor(state) {
    if (!state.worksiteResolution) {
      const waitingFor = state.maraChoice ? "Ivo's Worksite resolution" : "Mara and Ivo";
      elements.actorWorkspace.innerHTML = `<div class="waiting-state"><span>WAITING FOR GENERATED WORK</span><h2>No Mandate has been invented early.</h2><p>This client is waiting for ${waitingFor}. It will reconstruct the result from the shared event log, even after a reload or late join.</p><div class="pulse-line"></div></div>`;
      return;
    }
    const assigned = Boolean(state.acceptedMandate);
    elements.actorWorkspace.innerHTML = `
      <div class="scene-kicker"><span>PLAYER C · DOWNSTREAM CONSEQUENCE</span><em>${state.worksiteResolution}</em></div>
      <h2 class="scene-title">Ivo's local result became somebody else's beginning.</h2>
      <p class="scene-copy">These jobs were not available before Event 1. Accept one to prove the world continues without settling the season.</p>
      <div class="mandate-grid">${state.nextMandates.map((mandate) => `
        <article class="mandate-card ${mandate.status === "assigned" ? "assigned" : ""}">
          <header><span>${mandate.role}</span><em>${mandate.status.toUpperCase()}</em></header>
          <strong>${mandate.id}</strong><h3>${escapeHtml(mandate.title)}</h3>
          <button type="button" class="${mandate.status === "assigned" ? "ghost" : "primary"}" data-mandate="${mandate.id}" ${assigned ? "disabled" : ""}>${mandate.status === "assigned" ? "ACCEPTED BY NIA" : "ACCEPT MANDATE"}</button>
        </article>`).join("")}</div>
      ${assigned ? `<div class="continuation-proof"><b>EVENT 2 ACCEPTED</b><span>${state.acceptedMandate} changed from QUEUED → ASSIGNED.</span><em>SEASON ACTIVE · SETTLEMENT NOT STARTED</em></div>` : ""}`;
    elements.actorWorkspace.querySelectorAll("[data-mandate]").forEach((button) => {
      button.addEventListener("click", () => commitAction("MANDATE_ACCEPTED", { mandateId: button.dataset.mandate }));
    });
  }

  function renderObserver(state) {
    const stageCopy = {
      mara: ["Open Mara's URL", "Commit the first civic decision. Ivo is currently blocked."],
      ivo: ["Watch Ivo inherit", `Mara chose ${state.maraChoice}. Ivo now has only the two legal branch actions.`],
      successor: ["Watch work propagate", `${state.worksiteResolution} generated three Mandates for the next citizens.`],
      continuing: ["Handoff proved", `${state.acceptedMandate} is assigned while Season Zero remains active.`]
    }[state.stage];
    elements.actorWorkspace.innerHTML = `
      <div class="observer-hero">
        <span>READ-ONLY JUDGE VIEW</span><h2>${stageCopy[0]}</h2><p>${stageCopy[1]}</p>
        <div class="proof-thesis"><b>THE CLAIM</b><strong>A stranger's accepted action changes what another stranger can legally do next.</strong><small>Dialogue is authored in this build. State transitions are deterministic. Nothing here is a Solana transaction.</small></div>
      </div>
      <div class="observer-route">
        <span>OPEN EACH ROLE IN A SEPARATE TAB</span>
        <a href="${escapeHtml(currentUrl("mara"))}" target="_blank"><b>01</b><strong>Mara · Player A</strong><em>${state.maraChoice ? "COMMITTED" : "READY"}</em></a>
        <a href="${escapeHtml(currentUrl("ivo"))}" target="_blank"><b>02</b><strong>Ivo · Player B</strong><em>${state.ivoChoice ? "COMMITTED" : state.maraChoice ? "READY" : "BLOCKED"}</em></a>
        <a href="${escapeHtml(currentUrl("successor"))}" target="_blank"><b>03</b><strong>Nia · Player C</strong><em>${state.acceptedMandate ? "ASSIGNED" : state.worksiteResolution ? "READY" : "BLOCKED"}</em></a>
      </div>
      <div class="boundary-grid"><div><span>AI LAYER</span><strong>Memory expression</strong><em>Authored stand-in</em></div><div><span>RULE LAYER</span><strong>Allowed actions + effects</strong><em>Functional locally</em></div><div><span>CANON LAYER</span><strong>Hash-chained events</strong><em>Local, not Solana</em></div></div>`;
  }

  function renderInvariants(state) {
    const invariants = Core.invariantResults(state, verification);
    const passCount = invariants.filter((item) => item.pass).length;
    elements.invariantScore.textContent = `${passCount}/${invariants.length} PASS`;
    elements.invariantScore.classList.toggle("bad", passCount !== invariants.length);
    elements.invariantList.innerHTML = invariants.map((item) => `<div class="invariant ${item.pass ? "pass" : "fail"}"><i>${item.pass ? "✓" : "!"}</i><span>${escapeHtml(item.label)}</span></div>`).join("");
  }

  function eventTitle(event) {
    if (event.type === "MARA_CHOICE") return event.payload.choice === "oath" ? "GRAIN OATH BOUND" : "CHARTER INVOKED";
    if (event.type === "IVO_CHOICE") {
      const resolution = Core.RESOLUTIONS[event.payload && event.payload.choice];
      return resolution ? `${resolution.code} RESOLVED` : "UNKNOWN IVO ACTION";
    }
    if (event.type === "MANDATE_ACCEPTED") return `${event.payload && event.payload.mandateId ? event.payload.mandateId : "UNKNOWN MANDATE"} ACCEPTED`;
    return "UNKNOWN EVENT";
  }

  function renderEvents() {
    const events = Array.isArray(store.events) ? store.events : [];
    elements.eventCount.textContent = `${events.length} EVENT${events.length === 1 ? "" : "S"}`;
    if (!events.length) {
      elements.eventList.innerHTML = `<div class="empty-events"><i>0</i><span>Genesis is verified. Waiting for Mara's first accepted command.</span></div>`;
      return;
    }
    let prior = Core.initialState();
    elements.eventList.innerHTML = events.map((event) => {
      let after = prior;
      let diffs = [];
      try {
        after = Core.applyCommand(prior, {
          type: event.type,
          actorRole: event.actorRole,
          payload: event.payload,
          expectedSeq: event.seq,
          expectedPrevHash: event.prevHash
        });
        diffs = Core.stateDiff(prior, after).slice(0, 5);
      } catch {
        diffs = [{ path: "integrity", before: "valid", after: "failed" }];
      }
      prior = after;
      const person = Core.PEOPLE[event.actorRole] || { name: `Unknown actor · ${event.actorRole || "missing"}` };
      return `<article class="event-card">
        <header><b>${String(event.seq).padStart(2, "0")}</b><div><span>${escapeHtml(person.name)} · ${escapeHtml(event.ruleId || "UNKNOWN RULE")}</span><strong>${escapeHtml(eventTitle(event))}</strong></div><em>${verification.valid ? "VALID" : "HALTED"}</em></header>
        <div class="event-diff">${diffs.map((diff) => `<span><code>${escapeHtml(diff.path)}</code><del>${escapeHtml(pretty(diff.before))}</del><i>→</i><ins>${escapeHtml(pretty(diff.after))}</ins></span>`).join("")}</div>
        <footer><span>PREV ${shortHash(event.prevHash)}</span><strong>EVENT ${shortHash(event.eventHash)}</strong></footer>
      </article>`;
    }).join("");
  }

  function announcePresence() {
    if (!verification) return;
    const item = {
      type: "PRESENCE",
      role,
      clientId,
      finalStateHash: verification.finalStateHash,
      headEventHash: verification.headEventHash,
      valid: verification.valid,
      seenAt: Date.now()
    };
    presence.set(clientId, item);
    if (channel) channel.postMessage(item);
    renderPresence();
  }

  function renderPresence() {
    const cutoff = Date.now() - 7000;
    for (const [id, item] of presence.entries()) {
      if (item.seenAt < cutoff) presence.delete(id);
    }
    const byRole = new Map();
    Array.from(presence.values()).sort((a, b) => b.seenAt - a.seenAt).forEach((item) => {
      if (!byRole.has(item.role)) byRole.set(item.role, item);
    });
    const active = validRoles.flatMap((itemRole) => byRole.has(itemRole) ? [byRole.get(itemRole)] : []);
    const uniqueHeads = new Set(active.map((item) => `${item.headEventHash}:${item.finalStateHash}`));
    const allValid = active.every((item) => item.valid);
    const complete = active.length === 4;
    const match = uniqueHeads.size <= 1 && allValid;
    elements.agreementLabel.textContent = !match ? "DIVERGED" : complete ? "VERIFIED 4/4" : `MATCHING ${active.length}/4`;
    elements.agreementLabel.classList.toggle("bad", !match);
    elements.clientList.innerHTML = validRoles.map((itemRole) => {
      const item = byRole.get(itemRole);
      return `<div class="client ${item ? "online" : ""}"><i></i><span>${itemRole === "successor" ? "NEXT" : itemRole.toUpperCase()}</span><code>${item ? shortHash(item.headEventHash) : "NOT OPEN"}</code></div>`;
    }).join("");
  }

  function buildExport() {
    const state = verification.finalState;
    return {
      schemaVersion: Core.SCHEMA_VERSION,
      buildId: Core.BUILD_ID,
      sessionId,
      localProofDisclosure: Core.DISCLOSURE,
      seed: store.seed,
      genesisHash: verification.genesisHash,
      events: Core.clone(store.events),
      finalState: Core.clone(state),
      finalStateHash: verification.finalStateHash,
      headEventHash: verification.headEventHash,
      invariantResults: Core.invariantResults(state, verification),
      verification: { valid: verification.valid, errors: Core.clone(verification.errors) },
      exportedAt: new Date().toISOString()
    };
  }

  function exportProof() {
    if (!verification.valid) {
      showToast("Cannot export a proof whose integrity check failed", "error");
      return;
    }
    const blob = new Blob([`${JSON.stringify(buildExport(), null, 2)}\n`], { type: "application/json" });
    const href = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = href;
    link.download = `${sessionId}-session-proof.json`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(href), 1000);
    showToast("Exported replayable session-proof.json", "success");
  }

  function resetProof() {
    if (!window.confirm(`Reset only the local proof session “${sessionId}”?`)) return;
    const resetStore = newStore();
    saveStore(resetStore);
    if (channel) channel.postMessage({ type: "SESSION_UPDATED", sourceClientId: clientId, reset: true });
    refresh("reset");
    showToast("Local proof session reset", "success");
  }

  let toastTimer;
  function showToast(message, tone = "success") {
    clearTimeout(toastTimer);
    elements.toast.textContent = message;
    elements.toast.dataset.tone = tone;
    elements.toast.classList.add("visible");
    toastTimer = setTimeout(() => elements.toast.classList.remove("visible"), 3600);
  }

  document.getElementById("copy-link").addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(window.location.href);
      showToast("View link copied", "success");
    } catch {
      showToast("Clipboard unavailable; copy the browser address", "error");
    }
  });
  document.getElementById("export-proof").addEventListener("click", exportProof);
  document.getElementById("reset-proof").addEventListener("click", resetProof);

  window.addEventListener("storage", (event) => {
    if (event.key === storageKey) refresh("storage");
  });

  if (channel) {
    channel.addEventListener("message", (event) => {
      const message = event.data || {};
      if (message.type === "SESSION_UPDATED" && message.sourceClientId !== clientId) refresh("broadcast");
      if (message.type === "PRESENCE") {
        presence.set(message.clientId, message);
        renderPresence();
      }
      if (message.type === "PING" && message.sourceClientId !== clientId) announcePresence();
    });
  }

  setRoleNav();
  setInterval(() => {
    announcePresence();
    if (channel) channel.postMessage({ type: "PING", sourceClientId: clientId });
  }, 2500);

  window.__proofLab = {
    role,
    sessionId,
    clientId,
    storageKey,
    commitAction,
    refresh,
    buildExport,
    async getSnapshot() {
      await refresh("qa");
      return { store: Core.clone(store), verification: Core.clone(verification), role, clientId };
    }
  };

  refresh("initial").catch((error) => {
    console.error(error);
    showToast(error.message, "error");
  });
})();
