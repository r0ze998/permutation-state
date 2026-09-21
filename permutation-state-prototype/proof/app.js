(() => {
  "use strict";

  const Core = window.ProofCore;
  const params = new URLSearchParams(window.location.search);
  const validRoles = ["mara", "ivo", "successor", "observer"];
  const rawRole = params.get("role") || "observer";
  const role = validRoles.includes(rawRole) ? rawRole : "observer";
  const sessionId = sanitizeSession(params.get("session") || "aster-demo");
  const transport = params.get("transport") || "local";
  const magicBlockMode = transport === "magicblock";
  const storageKey = `permutation-state-proof:${sessionId}`;
  const channelName = `permutation-state-proof:${transport}:${sessionId}`;
  const storageOnlyTransport = transport === "storage";
  const clientKey = `permutation-state-proof-client:${transport}:${sessionId}:${role}`;
  const clientId = sessionStorage.getItem(clientKey) || crypto.randomUUID();
  sessionStorage.setItem(clientKey, clientId);

  const channel = !storageOnlyTransport && "BroadcastChannel" in window ? new BroadcastChannel(channelName) : null;
  const presence = new Map();
  let store = loadOrCreateStore();
  let verification = null;
  let renderRevision = 0;
  let networkSession = null;
  let gatewayHealth = null;
  let gatewayError = null;
  let actionPending = false;
  let pollInFlight = false;
  let lastRenderFingerprint = null;
  let lastActorFingerprint = null;
  let lastRenderedEventCount = null;
  let worldReactionTimer = null;

  const elements = {
    app: document.getElementById("proof-app"),
    disclosure: document.getElementById("transport-disclosure"),
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
    networkEvidence: document.getElementById("network-evidence"),
    networkLayer: document.getElementById("network-layer"),
    networkReceiptList: document.getElementById("network-receipt-list"),
    networkBoundary: document.getElementById("network-boundary"),
    eventList: document.getElementById("event-list"),
    observerActions: document.getElementById("observer-actions"),
    chronicleToggle: document.getElementById("chronicle-toggle"),
    chroniclePanel: document.getElementById("world-chronicle"),
    toast: document.getElementById("toast"),
    worldNoteKicker: document.getElementById("world-note-kicker"),
    worldNoteCopy: document.getElementById("world-note-copy"),
    guildStatus: document.getElementById("guild-status"),
    sluiceStatus: document.getElementById("sluice-status"),
    marketStatus: document.getElementById("market-status")
  };

  function setChronicle(open) {
    elements.app.dataset.chronicle = open ? "open" : "closed";
    elements.chronicleToggle.setAttribute("aria-expanded", open ? "true" : "false");
    elements.chroniclePanel.inert = !open;
    elements.chroniclePanel.setAttribute("aria-hidden", open ? "false" : "true");
  }

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
    if (magicBlockMode) return newStore();
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
    if (magicBlockMode) return store;
    try {
      const parsed = JSON.parse(localStorage.getItem(storageKey));
      if (parsed) store = parsed;
    } catch {
      store = { ...newStore(), integrityError: "Stored session is not valid JSON" };
    }
    return store;
  }

  function saveStore(nextStore) {
    if (!magicBlockMode) localStorage.setItem(storageKey, JSON.stringify(nextStore));
    store = nextStore;
  }

  function currentUrl(nextRole) {
    const url = new URL(window.location.href);
    url.search = "";
    url.searchParams.set("proof", "1");
    url.searchParams.set("session", sessionId);
    url.searchParams.set("role", nextRole);
    if (transport !== "local") url.searchParams.set("transport", transport);
    return url.toString();
  }

  async function api(path, options = {}) {
    const response = await fetch(path, {
      ...options,
      headers: { "Content-Type": "application/json", ...(options.headers || {}) },
      cache: "no-store"
    });
    let payload = null;
    try {
      payload = await response.json();
    } catch {
      throw new Error(`Gateway returned ${response.status} without valid JSON`);
    }
    if (!response.ok) throw new Error(payload.error || `Gateway request failed (${response.status})`);
    return payload;
  }

  function acceptNetworkSession(payload) {
    if (!payload || !payload.store) throw new Error("Gateway session response is incomplete");
    networkSession = payload;
    gatewayError = null;
    store = payload.store;
  }

  function pendingRequiredCheckpoint(session = networkSession) {
    const receipts = session?.network?.receipts || [];
    return [...receipts].reverse().find((receipt) => receipt.checkpointRequired && !receipt.checkpoint) || null;
  }

  async function loadGatewayHealth() {
    if (!magicBlockMode) return;
    try {
      gatewayHealth = await api("/api/magicblock/health");
      gatewayError = null;
    } catch (error) {
      gatewayError = error.message;
    }
  }

  async function pullNetworkSession() {
    const payload = await api(`/api/magicblock/session?session=${encodeURIComponent(sessionId)}`);
    acceptNetworkSession(payload);
  }

  function getRenderFingerprint(nextVerification) {
    const receipts = networkSession?.network?.receipts || [];
    const latestReceipt = receipts.at(-1) || null;
    const season = networkSession?.season || null;
    const economyReceipts = networkSession?.seasonNetwork?.economyReceipts
      || networkSession?.network?.economyReceipts
      || [];
    return JSON.stringify({
      valid: nextVerification.valid,
      errors: nextVerification.errors,
      stateHash: nextVerification.finalStateHash,
      headHash: nextVerification.headEventHash,
      initialized: networkSession?.initialized ?? null,
      delegated: networkSession?.delegated ?? null,
      gatewayError,
      receiptCount: receipts.length,
      checkpoint: latestReceipt?.checkpoint?.signature || null,
      checkpointError: latestReceipt?.checkpointError || null,
      seasonSeq: season?.seq ?? null,
      seasonHead: season?.headEventHash || null,
      seasonPurse: season?.purseTotal ?? null,
      economyReceiptCount: economyReceipts.length,
      economyReceiptHead: economyReceipts.at(-1)?.signature || null,
      seasonNetworkUpdatedAt: networkSession?.seasonNetwork?.updatedAt || null
    });
  }

  function getActorFingerprint(nextVerification) {
    const requiredCheckpoint = pendingRequiredCheckpoint(networkSession);
    return JSON.stringify({
      valid: nextVerification.valid,
      errors: nextVerification.errors,
      stateHash: nextVerification.finalStateHash,
      initialized: networkSession?.initialized ?? null,
      delegated: networkSession?.delegated ?? null,
      gatewayError,
      checkpointRequired: requiredCheckpoint?.er?.signature || requiredCheckpoint?.eventHash || null,
      checkpointError: requiredCheckpoint?.checkpointError || null
    });
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

  function formatMockUnits(value) {
    if (!Number.isFinite(Number(value))) return "—";
    return (Number(value) / 1_000_000).toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 6
    });
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
    if (magicBlockMode && source !== "network-response") {
      try {
        await pullNetworkSession();
      } catch (error) {
        gatewayError = error.message;
      }
    } else {
      reloadStore();
    }
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
    const fingerprint = getRenderFingerprint(nextVerification);
    const actorFingerprint = getActorFingerprint(nextVerification);
    const shouldRender = source !== "network-poll" || fingerprint !== lastRenderFingerprint;
    const shouldRenderActor = source !== "network-poll" || actorFingerprint !== lastActorFingerprint;
    const priorEventCount = lastRenderedEventCount;
    verification = nextVerification;
    if (shouldRender) {
      render(source, { renderActorPanel: shouldRenderActor });
      lastRenderFingerprint = fingerprint;
      if (shouldRenderActor) lastActorFingerprint = actorFingerprint;
      lastRenderedEventCount = nextVerification.validatedEventCount;
      if (priorEventCount !== null && nextVerification.validatedEventCount > priorEventCount) {
        playWorldReaction(store.events.at(-1));
      }
    }
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
    if (actionPending) return null;
    setActionPending(true);
    let submittedEvent = null;
    return withSessionLock(async () => {
      if (magicBlockMode) {
        await pullNetworkSession();
        if (!networkSession.initialized || !networkSession.delegated) {
          throw new Error("Initialize and delegate this shared MagicBlock session before playing");
        }
        if (pendingRequiredCheckpoint()) {
          throw new Error("Verify the required Solana checkpoint before the next player action");
        }
      } else {
        reloadStore();
      }
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
      submittedEvent = event;
      if (magicBlockMode) {
        showToast("Submitting signed action to the Ephemeral Rollup…", "success");
        const response = await api("/api/magicblock/action", {
          method: "POST",
          body: JSON.stringify({ session: sessionId, event })
        });
        acceptNetworkSession(response);
      } else {
        const nextStore = { ...store, events: [...store.events, event], updatedAt: new Date().toISOString() };
        saveStore(nextStore);
      }
      if (channel) channel.postMessage({ type: "SESSION_UPDATED", sourceClientId: clientId, eventHash: event.eventHash });
      await refresh(magicBlockMode ? "network-response" : "commit");
      const checkpointPending = Boolean(magicBlockMode && pendingRequiredCheckpoint());
      showToast(
        checkpointPending
          ? "The work changed Aster, but its Solana checkpoint still needs verification"
          : worldChangeMessage(event),
        checkpointPending ? "error" : "success"
      );
      return event;
    }).catch(async (error) => {
      if (magicBlockMode && submittedEvent) {
        try {
          await refresh("action-recovery");
          const accepted = store.events.some((event) => event.eventHash === submittedEvent.eventHash);
          if (accepted) {
            showToast(`${worldChangeMessage(submittedEvent)} · recovered after reconnect`, "success");
            return submittedEvent;
          }
        } catch {
          // Preserve the original submission error when recovery cannot establish the shared state.
        }
      }
      showToast(error.message, "error");
      throw error;
    }).finally(() => {
      setActionPending(false);
    });
  }

  function setActionPending(pending) {
    actionPending = pending;
    elements.app.dataset.pending = pending ? "true" : "false";
    if (pending) {
      elements.actorWorkspace.querySelectorAll("button").forEach((button) => {
        button.disabled = true;
      });
    } else if (verification) {
      renderActor(verification.finalState);
    }
  }

  function worldChangeMessage(event) {
    if (event?.type === "MARA_CHOICE") {
      return event.payload?.choice === "oath"
        ? "The Service Stair opened · Ivo can inherit Mara’s oath"
        : "The outer gate opened · Ivo inherits a sealed stair";
    }
    if (event?.type === "IVO_CHOICE") return "East Sluice changed · new civic work reached Aster";
    if (event?.type === "MANDATE_ACCEPTED") return `${event.payload?.mandateId || "A new Mandate"} now belongs to Nia`;
    return "Aster’s shared history changed";
  }

  function playWorldReaction(event) {
    if (!event) return;
    clearTimeout(worldReactionTimer);
    const reaction = event.type === "MARA_CHOICE"
      ? `mara-${event.payload?.choice || "choice"}`
      : event.type === "IVO_CHOICE"
        ? `ivo-${event.payload?.choice || "choice"}`
        : "mandate-accepted";
    elements.app.classList.remove("world-reacting");
    elements.app.dataset.reaction = reaction;
    void elements.app.offsetWidth;
    elements.app.classList.add("world-reacting");
    worldReactionTimer = setTimeout(() => {
      elements.app.classList.remove("world-reacting");
    }, 1400);
  }

  function render(source, { renderActorPanel = true } = {}) {
    const state = verification.finalState;
    elements.app.dataset.role = role;
    elements.app.dataset.transport = magicBlockMode ? "magicblock" : "local";
    elements.app.dataset.integrity = verification.valid ? "valid" : "invalid";
    elements.app.dataset.stage = state.stage || "mara";
    elements.app.dataset.branch = state.maraChoice || "unmade";
    elements.app.dataset.resolution = state.worksiteResolution || "unresolved";
    renderWorldScene(state);
    elements.disclosure.textContent = magicBlockMode
      ? "MAGICBLOCK LOCAL CLUSTER · REAL ER STATE · DISPOSABLE DEMO SIGNERS · NO REAL FUNDS"
      : Core.DISCLOSURE;
    elements.sessionLabel.textContent = sessionId;
    const networkHealthy = !magicBlockMode || (!gatewayError && networkSession);
    const checkpointBlocked = Boolean(magicBlockMode && pendingRequiredCheckpoint());
    elements.syncLabel.textContent = !networkHealthy
      ? "GATEWAY OFFLINE"
      : !verification.valid
        ? "INTEGRITY HALT"
        : checkpointBlocked
          ? "CHECKPOINT REQUIRED"
        : magicBlockMode
          ? (networkSession.initialized && networkSession.delegated
              ? "ER STATE SYNCED"
              : networkSession.initialized
                ? "ER DELEGATION NEEDED"
                : "NOT INITIALIZED")
          : (source === "storage" || source === "broadcast" ? "SYNCED NOW" : "REPLAY VERIFIED");
    elements.syncLabel.classList.toggle(
      "bad",
      !verification.valid
        || !networkHealthy
        || checkpointBlocked
        || Boolean(magicBlockMode && networkSession?.initialized && !networkSession?.delegated)
    );
    renderMetrics(state);
    renderWorldStatus(state);
    elements.stateHash.textContent = shortHash(verification.finalStateHash);
    elements.stateHash.title = verification.finalStateHash;
    elements.headHash.textContent = shortHash(verification.headEventHash);
    elements.headHash.title = verification.headEventHash;
    renderCausalStrip(state);
    if (renderActorPanel) renderActor(state);
    renderInvariants(state);
    renderNetworkEvidence();
    renderEvents();
    renderPresence();
    elements.observerActions.hidden = role !== "observer";
  }

  function renderWorldScene(state) {
    if (!elements.worldNoteKicker || !elements.worldNoteCopy || !elements.guildStatus || !elements.sluiceStatus || !elements.marketStatus) return;
    const resolution = state.ivoChoice;
    if (!state.maraChoice) {
      elements.worldNoteKicker.textContent = "NOW IN ASTER";
      elements.worldNoteCopy.textContent = "The East Sluice waits for an envoy’s word.";
      elements.guildStatus.textContent = "Tala is waiting";
      elements.sluiceStatus.textContent = "Gate held closed";
      elements.marketStatus.textContent = "3 citizens nearby";
      return;
    }
    if (!resolution) {
      const oath = state.maraChoice === "oath";
      elements.worldNoteKicker.textContent = "TALA REMEMBERS";
      elements.worldNoteCopy.textContent = oath
        ? "Riverkeepers are carrying Mara’s oath toward the Service Stair."
        : "The charter opened the outer gate, but the Service Stair was sealed.";
      elements.guildStatus.textContent = oath ? "Engineers departing" : "Riverkeepers withdrawn";
      elements.sluiceStatus.textContent = oath ? "Service Stair open" : "Service Stair locked";
      elements.marketStatus.textContent = "Ivo’s work is ready";
      return;
    }
    if (!state.acceptedMandate) {
      elements.worldNoteKicker.textContent = "THE WATER MOVED";
      elements.worldNoteCopy.textContent = resolution === "service"
        ? "The repaired sluice is carrying water—and three new Mandates—into Aster."
        : resolution === "reconcile"
          ? "Tala returned. A repaired civic route now carries work into the city."
          : "A new water route is open. Its consequences have become civic work.";
      elements.guildStatus.textContent = resolution === "reconcile" || resolution === "service" ? "Riverkeepers at work" : "Watching from afar";
      elements.sluiceStatus.textContent = "Water flowing";
      elements.marketStatus.textContent = "3 new Mandates";
      return;
    }
    elements.worldNoteKicker.textContent = "THE CIVILIZATION CONTINUES";
    elements.worldNoteCopy.textContent = `${state.acceptedMandate} has been claimed. Two needs still wait in Aster.`;
    elements.guildStatus.textContent = "Memory carried forward";
    elements.sluiceStatus.textContent = "Worksite stabilized";
    elements.marketStatus.textContent = "1 Mandate assigned";
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
    const season = magicBlockMode ? networkSession?.season : null;
    elements.worldStatus.innerHTML = `
      <div><span>SEASON</span><strong>${pretty(state.seasonStatus)}</strong></div>
      <div><span>WORKSITE 31</span><strong>${pretty(state.worksiteResolution || state.worksiteStatus)}</strong></div>
      <div><span>FOOD DEBT</span><strong>${state.foodDebt}</strong></div>
      <div><span>SETTLEMENT</span><strong>${pretty(state.settlementStatus)}</strong></div>
      <div><span>CLAIM</span><strong>${state.claimAvailable ? "AVAILABLE" : "UNAVAILABLE"}</strong></div>
      <div><span>STAGE</span><strong>${pretty(state.stage)}</strong></div>
      ${season ? `<div><span>VERIFIED PURSE</span><strong>${formatMockUnits(season.purseTotal)} MOCK USDC</strong></div>` : ""}`;
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
    elements.actorMode.textContent = magicBlockMode
      ? (role === "observer" ? "WITNESS MODE · ER" : "PLAYING · DEMO SIGNER")
      : (role === "observer" ? "WITNESS MODE · READ ONLY" : "YOU ARE PLAYING");

    if (magicBlockMode && (!networkSession || gatewayError)) {
      renderGatewayOffline();
      return;
    }
    if (magicBlockMode && (!networkSession.initialized || !networkSession.delegated)) {
      renderNetworkBootstrap();
      return;
    }
    if (!verification.valid) {
      elements.actorWorkspace.innerHTML = `<div class="integrity-halt"><span>FAIL CLOSED</span><h2>Proof log integrity failed.</h2><p>No client may append a new event until this local session is reset.</p><code>${escapeHtml(verification.errors.join(" · "))}</code></div>`;
      return;
    }
    if (magicBlockMode && role !== "observer" && pendingRequiredCheckpoint()) {
      renderCheckpointHalt();
      return;
    }
    if (role === "mara") renderMara(state);
    if (role === "ivo") renderIvo(state);
    if (role === "successor") renderSuccessor(state);
    if (role === "observer") renderObserver(state);
  }

  function renderCheckpointHalt() {
    const receipt = pendingRequiredCheckpoint();
    elements.actorWorkspace.innerHTML = `
      <div class="integrity-halt">
        <span>SOLANA CHECKPOINT REQUIRED</span>
        <h2>The shared world is paused before the next handoff.</h2>
        <p>Ivo's action is already accepted on the Ephemeral Rollup, but its required Solana checkpoint has not been verified. Nia receives no playable action until the read-back succeeds.</p>
        <code>${escapeHtml(receipt?.checkpointError || "Checkpoint read-back is pending")}</code>
        <p>Use RETRY SOLANA CHECKPOINT in the evidence rail. The accepted ER event will not be submitted again.</p>
      </div>`;
  }

  function renderGatewayOffline() {
    const fallbackUrl = new URL(window.location.href);
    fallbackUrl.searchParams.delete("transport");
    elements.actorWorkspace.innerHTML = `
      <div class="network-gate offline">
        <span>MAGICBLOCK GATEWAY UNAVAILABLE</span>
        <h2>The shared world is not reachable yet.</h2>
        <p>${escapeHtml(gatewayError || "Waiting for the local gateway to report its state.")}</p>
        <button type="button" class="ghost" id="retry-gateway">RETRY CONNECTION</button>
        <a class="ghost link-button" href="${escapeHtml(fallbackUrl.toString())}">OPEN BROWSER-LOCAL FALLBACK</a>
      </div>`;
    document.getElementById("retry-gateway").addEventListener("click", () => {
      refresh("retry").catch((error) => showToast(error.message, "error"));
    });
  }

  function renderNetworkBootstrap() {
    const resumeDelegation = Boolean(networkSession?.initialized && !networkSession?.delegated);
    elements.actorWorkspace.innerHTML = `
      <div class="network-gate">
        <span>${resumeDelegation ? "BASE ACCOUNT READY · ER HANDOFF INCOMPLETE" : "NEW SHARED CIVILIZATION"}</span>
        <h2>${resumeDelegation ? "Resume delegation to MagicBlock." : "Open East Sluice on MagicBlock."}</h2>
        <p>${resumeDelegation
          ? "The Worksite already exists on the local Solana base layer, but it is not delegated to the Ephemeral Rollup yet. Resume the handoff before any player action is enabled."
          : "This creates the Worksite account on the local Solana base layer, then delegates it to the Ephemeral Rollup. Every role URL will read the same shared account."}</p>
        <dl>
          <div><dt>SESSION</dt><dd>${escapeHtml(sessionId)}</dd></div>
          <div><dt>PROGRAM</dt><dd>${escapeHtml(shortHash(gatewayHealth?.programId || networkSession.descriptor?.programId))}</dd></div>
          <div><dt>FUNDS</dt><dd>DISPOSABLE LOCALNET ONLY</dd></div>
        </dl>
        <button type="button" class="primary" id="bootstrap-network" ${actionPending ? "disabled" : ""}>${resumeDelegation ? "RESUME ER DELEGATION" : "INITIALIZE + DELEGATE WORKSITE"}</button>
        <small>The local gateway temporarily holds the three disposable demo signers. Role URLs are presentation only, not authorization; production wallets and player session keys are not implemented in this slice.</small>
      </div>`;
    document.getElementById("bootstrap-network").addEventListener("click", bootstrapNetwork);
  }

  async function bootstrapNetwork() {
    if (actionPending) return;
    const resumeDelegation = Boolean(networkSession?.initialized && !networkSession?.delegated);
    setActionPending(true);
    showToast(resumeDelegation ? "Resuming ER delegation…" : "Creating and delegating the shared Worksite…", "success");
    try {
      const response = await api("/api/magicblock/bootstrap", {
        method: "POST",
        body: JSON.stringify({ session: sessionId })
      });
      acceptNetworkSession(response);
      if (!response.initialized || !response.delegated) {
        await refresh("network-response");
        showToast("The Worksite is not delegated yet; retry the ER handoff", "error");
        return;
      }
      if (channel) channel.postMessage({ type: "SESSION_UPDATED", sourceClientId: clientId, bootstrap: true });
      await refresh("network-response");
      showToast("Worksite delegated · real-time play is ready", "success");
    } catch (error) {
      showToast(error.message, "error");
      await refresh("retry");
    } finally {
      setActionPending(false);
      renderNetworkEvidence();
    }
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
        <div class="scene-kicker"><span>MARA · ENVOY AT EAST SLUICE</span><em>ASTER'S FIRST MOVE</em></div>
        <h2 class="scene-title">Tala guards the only safe road beneath the river.</h2>
        <p class="scene-copy">Aster needs the lower gate opened before nightfall. Decide what kind of relationship the city will carry into the work.</p>
        <blockquote>“Bring me a promise the city must remember—or bring me the old law.” <cite>— Tala, River Guild</cite></blockquote>
        <div class="choice-grid">
          ${choiceCard({ id: "oath", eyebrow: "RELATIONAL ROUTE", title: "Bind the Grain Oath", body: "Promise twelve crates after harvest. Tala keeps her engineers and opens the service stair.", facts: ["FOOD DEBT +12", "TRUST +20", "STAIR OPEN"] })}
          ${choiceCard({ id: "charter", eyebrow: "LEGAL ROUTE", title: "Invoke the Founding Charter", body: "Compel access without a grain promise. Tala withdraws her engineers and seals the stair.", facts: ["NO DEBT", "TRUST −20", "STAIR LOCKED"] })}
        </div>`;
      elements.actorWorkspace.querySelectorAll("[data-choice]").forEach((button) => {
        button.addEventListener("click", () => {
          button.classList.add("committing");
          button.setAttribute("aria-pressed", "true");
          commitAction("MARA_CHOICE", { choice: button.dataset.choice }).catch(() => {});
        });
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
      <p class="waiting-copy">The shared world has moved. Ivo can play immediately, and will inherit only the actions this history unlocked.</p>
      <a class="primary link-button handoff-cta" href="${escapeHtml(currentUrl("ivo"))}"><span>WORLD CHANGED</span>CONTINUE AS IVO — INHERIT MARA'S CHOICE →</a>`;
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
        <div class="waiting-state"><span>WAITING FOR MARA</span><h2>Ivo's road has not been written yet.</h2><p>When the envoy reaches terms with Tala, the consequences will arrive here automatically and reveal what the Maker can do.</p><div class="pulse-line"></div></div>`;
      return;
    }
    if (!state.ivoChoice) {
      const oath = state.maraChoice === "oath";
      elements.actorWorkspace.innerHTML = `
        ${ivoRibbon(state)}
        <div class="scene-kicker"><span>IVO · MAKER IN AN INHERITED WORLD</span><em>${state.memoryReceipt}</em></div>
        <h2 class="scene-title">Another citizen's promise has changed the road before you.</h2>
        <p class="scene-copy">Tala remembers how Mara approached her. That memory now decides which repairs are truly possible at the sluice.</p>
        <blockquote>“${oath ? "Mara spoke for Aster. Twelve crates after harvest. I opened the service stair for you, Maker." : "Your envoy brought law, not trust. The city may have its water. It will not have my help."}” <cite>— Tala, inherited memory</cite></blockquote>
        <div class="choice-grid">
          ${oath
            ? choiceCard({ id: "service", eyebrow: "UNLOCKED BY MARA'S OATH", title: "Repair through Service Stair", body: "Work beside the Riverkeepers and honor the debt that opened this path.", facts: ["WATER +24", "TIMBER −6", "R31-1"] })
              + choiceCard({ id: "millrace", eyebrow: "AVAILABLE FALLBACK", title: "Reopen the Old Millrace", body: "Bypass Tala's engineers without erasing Mara's promise.", facts: ["WATER +18", "TIMBER −3", "R31-2"] })
            : choiceCard({ id: "cut", eyebrow: "CHARTER-ONLY ROUTE", title: "Cut through the Sealed Millrace", body: "Restore water without Riverkeeper help and deepen the fracture.", facts: ["WATER +22", "COHESION −5", "R31-3"] })
              + choiceCard({ id: "reconcile", eyebrow: "CHARTER-ONLY ROUTE", title: "Return and Reconcile", body: "Negotiate a smaller obligation and reopen the service stair.", facts: ["FOOD DEBT +8", "TRUST +25", "R31-4"] })}
        </div>`;
      elements.actorWorkspace.querySelectorAll("[data-choice]").forEach((button) => {
        button.addEventListener("click", () => {
          button.classList.add("committing");
          button.setAttribute("aria-pressed", "true");
          commitAction("IVO_CHOICE", { choice: button.dataset.choice }).catch(() => {});
        });
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
      <a class="primary link-button handoff-cta" href="${escapeHtml(currentUrl("successor"))}"><span>NEW CIVIC WORK CREATED</span>CONTINUE AS NIA — CARRY THE RIPPLE FORWARD →</a>`;
  }

  function renderSuccessor(state) {
    if (!state.worksiteResolution) {
      const waitingFor = state.maraChoice ? "Ivo's Worksite resolution" : "Mara and Ivo";
      elements.actorWorkspace.innerHTML = `<div class="waiting-state"><span>THE CITY IS STILL CHANGING</span><h2>Nia's next calling does not exist yet.</h2><p>This view is waiting for ${waitingFor}. When the work is done, its consequences will become real jobs for Aster's next citizens.</p><div class="pulse-line"></div></div>`;
      return;
    }
    const assigned = Boolean(state.acceptedMandate);
    elements.actorWorkspace.innerHTML = `
      <div class="scene-kicker"><span>NIA · THE NEXT CITIZEN</span><em>${state.worksiteResolution}</em></div>
      <h2 class="scene-title">Yesterday's repair has become today's calling.</h2>
      <p class="scene-copy">These Mandates did not exist before Ivo finished the work. Choose one and carry Aster's shared history forward.</p>
      <div class="mandate-grid">${state.nextMandates.map((mandate) => `
        <article class="mandate-card ${mandate.status === "assigned" ? "assigned" : ""}">
          <header><span>${mandate.role}</span><em>${mandate.status.toUpperCase()}</em></header>
          <strong>${mandate.id}</strong><h3>${escapeHtml(mandate.title)}</h3>
          <button type="button" class="${mandate.status === "assigned" ? "ghost" : "primary"}" data-mandate="${mandate.id}" ${assigned ? "disabled" : ""}>${mandate.status === "assigned" ? "ACCEPTED BY NIA" : "ACCEPT MANDATE"}</button>
        </article>`).join("")}</div>
      ${assigned ? `<div class="continuation-proof"><b>EVENT 2 ACCEPTED</b><span>${state.acceptedMandate} changed from QUEUED → ASSIGNED.</span><em>SEASON ACTIVE · SETTLEMENT NOT STARTED</em></div>` : ""}`;
    elements.actorWorkspace.querySelectorAll("[data-mandate]").forEach((button) => {
      button.addEventListener("click", () => {
        button.classList.add("committing");
        button.setAttribute("aria-pressed", "true");
        commitAction("MANDATE_ACCEPTED", { mandateId: button.dataset.mandate }).catch(() => {});
      });
    });
  }

  function renderObserver(state) {
    const checkpointBlocked = Boolean(magicBlockMode && pendingRequiredCheckpoint());
    const stageCopy = {
      mara: ["The East Sluice waits for its envoy", "Mara must decide what kind of promise Aster will make. Ivo's future is still unwritten."],
      ivo: ["Tala remembers", `Mara chose ${state.maraChoice}. The memory has opened one future for Ivo and closed another.`],
      successor: ["The repair becomes a ripple", `${state.worksiteResolution} has created three new Mandates for Aster's citizens.`],
      continuing: ["Aster carries the choice forward", `${state.acceptedMandate} now belongs to Nia, while Season Zero continues around her.`]
    }[state.stage];
    const thesisBoundary = magicBlockMode
      ? "Dialogue is authored in this build. Rules are deterministic. Actions execute on a real local MagicBlock ER; Ivo's resolution requests a Solana base-layer checkpoint."
      : "Dialogue is authored in this build. State transitions are deterministic. Nothing here is a Solana transaction.";
    elements.actorWorkspace.innerHTML = `
      <div class="observer-hero">
        <span>WITNESSING THE SHARED WORLD</span><h2>${stageCopy[0]}</h2><p>${stageCopy[1]}</p>
        <div class="proof-thesis"><b>WHAT MAKES ASTER ALIVE</b><strong>One citizen's choice changes what another citizen can truly do next.</strong><small>${thesisBoundary}</small></div>
      </div>
      <div class="observer-route">
        <span>FOLLOW EACH CITIZEN'S PART OF THE STORY</span>
        <a href="${escapeHtml(currentUrl("mara"))}" target="_blank"><b>01</b><strong>Mara · Envoy</strong><em>${state.maraChoice ? "REMEMBERED" : "READY"}</em></a>
        <a href="${escapeHtml(currentUrl("ivo"))}" target="_blank"><b>02</b><strong>Ivo · Maker</strong><em>${state.ivoChoice ? "REMEMBERED" : state.maraChoice ? "READY" : "AWAITING MARA"}</em></a>
        <a href="${escapeHtml(currentUrl("successor"))}" target="_blank"><b>03</b><strong>Nia · Successor</strong><em>${checkpointBlocked ? "CHECKPOINT WAIT" : state.acceptedMandate ? "ANSWERED" : state.worksiteResolution ? "READY" : "AWAITING IVO"}</em></a>
      </div>
      <div class="boundary-grid"><div><span>AI LAYER</span><strong>Memory expression</strong><em>Authored stand-in</em></div><div><span>RULE LAYER</span><strong>Allowed actions + effects</strong><em>Deterministic program</em></div><div><span>CANON LAYER</span><strong>${magicBlockMode ? "ER state + checkpoint" : "Hash-chained events"}</strong><em>${magicBlockMode ? "Localnet, real transactions" : "Local, not Solana"}</em></div></div>
      ${magicBlockMode ? `<p class="signer-boundary"><b>DEMO SECURITY BOUNDARY</b> Disposable localnet signers are held by the gateway for this playable slice. Production wallets and player session keys are not implemented.</p>` : ""}`;
  }

  function allInvariantResults(state) {
    const invariants = Core.invariantResults(state, verification);
    if (magicBlockMode && networkSession?.initialized) {
      const chain = networkSession?.state;
      const receipts = networkSession?.network?.receipts || [];
      const latestEr = receipts.at(-1)?.er || null;
      const requiredCheckpoint = pendingRequiredCheckpoint();
      const checkpoint = requiredCheckpoint
        ? null
        : receipts.map((item) => item.checkpoint).filter(Boolean).at(-1);
      const expectsCheckpoint = Boolean(state.ivoChoice);
      invariants.push(
        { label: "Worksite is delegated to the Ephemeral Rollup", pass: networkSession?.delegated === true },
        { label: "ER account state root recomputes exactly", pass: Boolean(chain?.stateRootMatches) },
        { label: "ER sequence equals replayed event count", pass: Boolean(chain) && chain.seq === store.events.length }
      );
      if (latestEr) {
        invariants.push(
          { label: "Read-back ER head matches latest ER receipt", pass: chain?.headEventHash === latestEr.headEventHash },
          { label: "Read-back ER sequence matches latest ER receipt", pass: chain?.seq === latestEr.sequence },
          { label: "Read-back ER root matches latest ER receipt", pass: chain?.stateRoot === latestEr.stateRoot }
        );
      } else {
        invariants.push({ label: "Genesis has no player ER receipt", pass: Boolean(chain) && chain.seq === 0 && store.events.length === 0 });
      }
      if (expectsCheckpoint) {
        invariants.push({
          label: "Ivo result read back from Solana checkpoint",
          pass: checkpoint?.settlement === "checkpoint-read-back-verified"
            && checkpoint.stateRoot === receipts.find((item) => item.checkpoint === checkpoint)?.er?.stateRoot
        });
      }
      const season = networkSession?.season;
      invariants.push(
        { label: "Season PDA state root recomputes", pass: Boolean(season?.stateRootMatches) },
        { label: "Season remains ACTIVE", pass: season?.status === 1 },
        { label: "Season claim remains unavailable", pass: season?.status === 1 && season?.claimableUnits === 0 && season?.claimCount === 0 },
        { label: "Purse ledger allocations reconcile", pass: Boolean(season)
          && season.purseTotal === season.entryPurseUnits + season.marketplacePurseUnits
          && season.entryGrossUnits + season.marketplaceGrossUnits
            === season.purseTotal + season.opsUnits + season.sellerUnits }
      );
    }
    return invariants;
  }

  function renderInvariants(state) {
    const invariants = allInvariantResults(state);
    const passCount = invariants.filter((item) => item.pass).length;
    elements.invariantScore.textContent = `${passCount}/${invariants.length} PASS`;
    elements.invariantScore.classList.toggle("bad", passCount !== invariants.length);
    elements.invariantList.innerHTML = invariants.map((item) => `<div class="invariant ${item.pass ? "pass" : "fail"}"><i>${item.pass ? "✓" : "!"}</i><span>${escapeHtml(item.label)}</span></div>`).join("");
  }

  function signatureMarkup(receipt) {
    if (!receipt?.signature) return "—";
    const label = escapeHtml(shortHash(receipt.signature));
    return receipt.explorerUrl
      ? `<a href="${escapeHtml(receipt.explorerUrl)}" target="_blank" rel="noreferrer">${label} ↗</a>`
      : `<code title="${escapeHtml(receipt.signature)}">${label}</code>`;
  }

  function renderNetworkEvidence() {
    elements.networkEvidence.hidden = !magicBlockMode;
    if (!magicBlockMode) return;

    const receipts = networkSession?.network?.receipts || [];
    const latest = receipts.at(-1) || null;
    const latestEr = latest?.er || null;
    const requiredCheckpoint = pendingRequiredCheckpoint();
    const verifiedCheckpointReceipt = [...receipts].reverse().find((item) => item.checkpoint) || null;
    const checkpoint = requiredCheckpoint ? null : verifiedCheckpointReceipt?.checkpoint || null;
    const checkpointError = requiredCheckpoint
      ? requiredCheckpoint.checkpointError || null
      : [...receipts].reverse().find((item) => item.checkpointError)?.checkpointError || null;
    const descriptor = networkSession?.descriptor;
    const chain = networkSession?.state;
    const season = networkSession?.season;
    const entryOps = season ? season.entryGrossUnits - season.entryPurseUnits : 0;
    const marketplaceOps = season
      ? season.marketplaceGrossUnits - season.marketplacePurseUnits - season.sellerUnits
      : 0;
    const economyReceipts = networkSession?.seasonNetwork?.economyReceipts
      || networkSession?.network?.economyReceipts
      || [];
    elements.networkLayer.textContent = gatewayError
      ? "OFFLINE"
      : !networkSession?.initialized
        ? "NOT INITIALIZED"
        : networkSession.delegated
          ? "ER DELEGATED"
          : "BASE LAYER";
    elements.networkLayer.classList.toggle("bad", Boolean(gatewayError));

    const erBody = latestEr
      ? `<strong>${signatureMarkup(latestEr)}</strong><span>SEQ ${latestEr.sequence ?? "—"} · SLOT ${latestEr.slot ?? "—"}</span><code title="${escapeHtml(latestEr.stateRoot || "")}">ROOT ${escapeHtml(shortHash(latestEr.stateRoot))}</code><em>FAST CONFIRMATION · NOT YET SOLANA SETTLEMENT</em>`
      : networkSession?.initialized && networkSession?.delegated
        ? `<strong>WAITING FOR FIRST PLAYER ACTION</strong><span>The delegated Worksite is ready for Mara.</span><em>NO ER TRANSACTION YET</em>`
        : networkSession?.initialized
          ? `<strong>ER DELEGATION INCOMPLETE</strong><span>The base-layer Worksite exists, but player actions stay locked until delegation succeeds.</span><em>RESUME DELEGATION</em>`
          : `<strong>WORKSITE NOT INITIALIZED</strong><span>Initialize and delegate this session before sending an action.</span><em>NO ER TRANSACTION YET</em>`;
    let checkpointBody = `<strong>POLICY: AFTER IVO RESOLUTION</strong><span>No base-layer gameplay checkpoint is expected yet.</span><em>NOT REQUESTED</em>`;
    let checkpointClass = "pending";
    const retryCheckpointButton = requiredCheckpoint
      ? `<button type="button" class="ghost compact" id="retry-checkpoint" ${actionPending ? "disabled" : ""}>${actionPending ? "RETRYING CHECKPOINT…" : "RETRY SOLANA CHECKPOINT"}</button>`
      : "";
    if (checkpoint) {
      checkpointClass = "verified";
      checkpointBody = `<strong>${signatureMarkup(checkpoint)}</strong><span>SEQ ${checkpoint.sequence ?? "—"} · SLOT ${checkpoint.slot ?? "—"}</span><code title="${escapeHtml(checkpoint.stateRoot || "")}">ROOT ${escapeHtml(shortHash(checkpoint.stateRoot))}</code><em>SOLANA READ-BACK VERIFIED</em>`;
    } else if (checkpointError) {
      checkpointClass = "failed";
      checkpointBody = `<strong>CHECKPOINT NOT VERIFIED</strong><span>${escapeHtml(checkpointError)}</span><em>ER RECEIPT IS NOT LABELED AS SOLANA</em>${retryCheckpointButton}`;
    } else if (requiredCheckpoint) {
      checkpointBody = `<strong>AWAITING VERIFIED READ-BACK</strong><span>Ivo's ER action is accepted. The next handoff stays locked until the base account matches this root.</span><em>REQUIRED · PENDING</em>${retryCheckpointButton}`;
    } else if (verification?.finalState?.ivoChoice) {
      checkpointBody = `<strong>AWAITING VERIFIED READ-BACK</strong><span>The UI will only label this settled after the base account matches the ER root.</span><em>PENDING</em>`;
    }

    const economyReceiptMarkup = economyReceipts.length
      ? economyReceipts.map((receipt) => `<span>${escapeHtml(receipt.sourceKind?.toUpperCase() || "SEASON")} · ${signatureMarkup(receipt)} · +${formatMockUnits(receipt.purseUnits)} PURSE</span>`).join("")
      : "<span>Ledger state was read directly from the Season PDA; this Worksite has no locally indexed credit receipts.</span>";
    const seasonBody = season
      ? `<strong>${formatMockUnits(season.purseTotal)} MOCK USDC</strong>
        <span>ENTRY ${formatMockUnits(season.entryGrossUnits)} → PURSE ${formatMockUnits(season.entryPurseUnits)} · OPS ${formatMockUnits(entryOps)}</span>
        <span>MARKET ${formatMockUnits(season.marketplaceGrossUnits)} → PURSE ${formatMockUnits(season.marketplacePurseUnits)} · OPS ${formatMockUnits(marketplaceOps)} · SELLER ${formatMockUnits(season.sellerUnits)}</span>
        <code title="${escapeHtml(season.headEventHash || "")}">LEDGER SEQ ${season.seq} · HEAD ${escapeHtml(shortHash(season.headEventHash))}</code>
        ${economyReceiptMarkup}
        <em>ACTIVE · SETTLEMENT NOT STARTED · CLAIM UNAVAILABLE · ONCHAIN MOCK LEDGER</em>`
      : `<strong>SEASON PDA NOT INITIALIZED</strong><span>No verified purse state is available.</span><em>NOT VERIFIED</em>`;

    elements.networkReceiptList.innerHTML = `
      <article class="layer-receipt er"><header><b>REAL-TIME ER TRANSACTION</b><i>${latestEr ? "ACCEPTED" : "WAITING"}</i></header>${erBody}</article>
      <article class="layer-receipt checkpoint ${checkpointClass}"><header><b>SOLANA CHECKPOINT</b><i>${checkpoint ? "VERIFIED" : checkpointError ? "FAILED" : "WAITING"}</i></header>${checkpointBody}</article>
      <article class="layer-receipt checkpoint ${season ? "verified" : "failed"}"><header><b>SEASON PURSE · SOLANA</b><i>${season ? "VERIFIED" : "MISSING"}</i></header>${seasonBody}</article>`;
    elements.networkBoundary.innerHTML = `
      <span>PROGRAM <code title="${escapeHtml(descriptor?.programId || "")}">${escapeHtml(shortHash(descriptor?.programId))}</code></span>
      <span>WORKSITE <code title="${escapeHtml(descriptor?.worksitePda || "")}">${escapeHtml(shortHash(descriptor?.worksitePda))}</code></span>
      <span>SEASON PDA <code title="${escapeHtml(descriptor?.seasonPurse || "")}">${escapeHtml(shortHash(descriptor?.seasonPurse))}</code></span>
      <span>ER SEQ <code>${chain?.seq ?? "—"}</code></span>
      <small>The verified onchain mock ledger is separate from the 348.50 MOCK USDC world projection in the visual prototype. Disposable local demo signers are gateway-held. Role URLs are presentation only, not authorization.</small>`;
    const retryButton = document.getElementById("retry-checkpoint");
    if (retryButton) retryButton.addEventListener("click", retrySolanaCheckpoint);
  }

  async function retrySolanaCheckpoint() {
    if (actionPending || !pendingRequiredCheckpoint()) return;
    actionPending = true;
    elements.app.dataset.pending = "true";
    const button = document.getElementById("retry-checkpoint");
    if (button) {
      button.disabled = true;
      button.textContent = "RETRYING CHECKPOINT…";
    }
    showToast("Retrying the required Solana checkpoint…", "success");
    try {
      const response = await api("/api/magicblock/checkpoint", {
        method: "POST",
        body: JSON.stringify({ session: sessionId })
      });
      acceptNetworkSession(response);
      if (channel) channel.postMessage({ type: "SESSION_UPDATED", sourceClientId: clientId, checkpoint: true });
      await refresh("network-response");
      const unresolved = pendingRequiredCheckpoint(response);
      if (unresolved) {
        showToast(unresolved.checkpointError || "Checkpoint is still pending", "error");
      } else {
        showToast("Solana checkpoint verified · next handoff unlocked", "success");
      }
    } catch (error) {
      showToast(error.message, "error");
      await refresh("retry");
    } finally {
      setActionPending(false);
      renderNetworkEvidence();
    }
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
      localProofDisclosure: store.localProofDisclosure || Core.DISCLOSURE,
      networkDisclosure: magicBlockMode
        ? (networkSession?.disclosure || "MAGICBLOCK NETWORK EVIDENCE · SEE NETWORK RECEIPTS AND CLUSTER")
        : null,
      seed: store.seed,
      genesisHash: verification.genesisHash,
      events: Core.clone(store.events),
      finalState: Core.clone(state),
      finalStateHash: verification.finalStateHash,
      headEventHash: verification.headEventHash,
      invariantResults: allInvariantResults(state),
      network: magicBlockMode && networkSession ? Core.clone({
        cluster: networkSession.cluster,
        descriptor: networkSession.descriptor,
        state: networkSession.state,
        stateLayer: networkSession.stateLayer,
        season: networkSession.season,
        seasonSlot: networkSession.seasonSlot,
        seasonNetwork: networkSession.seasonNetwork,
        delegated: networkSession.delegated,
        receipts: networkSession.network?.receipts || [],
        economyReceipts: networkSession.network?.economyReceipts || []
      }) : null,
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
    if (magicBlockMode) {
      if (!window.confirm("Start a fresh MagicBlock session? The current shared account and its receipts will remain intact.")) return;
      const nextSession = sanitizeSession(`aster-${Date.now().toString(36)}-${crypto.randomUUID().slice(0, 6)}`);
      const url = new URL(window.location.href);
      url.searchParams.set("session", nextSession);
      url.searchParams.set("transport", "magicblock");
      window.location.assign(url);
      return;
    }
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
  elements.chronicleToggle.addEventListener("click", () => {
    setChronicle(elements.app.dataset.chronicle !== "open");
  });
  document.querySelector("[data-chronicle-close]").addEventListener("click", () => setChronicle(false));
  if (magicBlockMode) document.getElementById("reset-proof").textContent = "START A FRESH SHARED SESSION";

  window.addEventListener("storage", (event) => {
    if (event.key === storageKey) refresh("storage");
  });

  if (channel) {
    channel.addEventListener("message", (event) => {
      const message = event.data || {};
      if (message.type === "SESSION_UPDATED" && message.sourceClientId !== clientId) {
        refresh(magicBlockMode ? "network-broadcast" : "broadcast").catch((error) => {
          gatewayError = error.message;
        });
      }
      if (message.type === "PRESENCE") {
        presence.set(message.clientId, message);
        renderPresence();
      }
      if (message.type === "PING" && message.sourceClientId !== clientId) announcePresence();
    });
  }

  setRoleNav();
  setChronicle(role === "observer");
  setInterval(() => {
    announcePresence();
    if (channel) channel.postMessage({ type: "PING", sourceClientId: clientId });
  }, 2500);

  if (magicBlockMode) {
    setInterval(async () => {
      if (pollInFlight || actionPending || document.hidden) return;
      pollInFlight = true;
      try {
        await refresh("network-poll");
      } catch (error) {
        gatewayError = error.message;
        elements.syncLabel.textContent = "SYNC RETRYING";
        elements.syncLabel.classList.add("bad");
      } finally {
        pollInFlight = false;
      }
    }, 1250);
  }

  window.__proofLab = {
    role,
    sessionId,
    clientId,
    transport,
    storageKey,
    commitAction,
    refresh,
    buildExport,
    async getSnapshot() {
      await refresh("qa");
      return {
        store: Core.clone(store),
        verification: Core.clone(verification),
        networkSession: networkSession ? Core.clone(networkSession) : null,
        role,
        clientId,
        transport
      };
    }
  };

  (async () => {
    await loadGatewayHealth();
    await refresh("initial");
  })().catch((error) => {
    console.error(error);
    showToast(error.message, "error");
  });
})();
