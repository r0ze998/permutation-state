(() => {
  "use strict";

  const STORAGE_KEY = "permutation-state-living-atlas-v2";
  const queryParams = new URLSearchParams(window.location.search);
  const previewCode = queryParams.get("preview");
  const judgeMode = queryParams.get("judge") === "1";

  const initialState = () => ({
    view: "pulse",
    drawerOpen: true,
    currentCitizen: "mara",
    flow: "pulse_a",
    aChoice: null,
    bChoice: null,
    pendingChoice: null,
    water: 28,
    food: 46,
    cohesion: 54,
    timber: 18,
    prosperity: 38,
    foodDebt: 0,
    talaTrust: 0,
    serviceStair: "closed",
    memoryReceipt: null,
    worksiteStatus: "active",
    worksiteResolution: null,
    seasonStatus: "active",
    seasonOutcome: "unresolved",
    epoch: 3,
    epochCount: 4,
    purseStatus: "accumulating",
    settlementStatus: "not_started",
    nextMandates: [],
    updatedAt: Date.now()
  });

  const branchCopy = {
    oath: {
      title: "Bind the Grain Oath",
      quote: "Twelve grain crates after harvest.",
      summary: "Tala opens the service stair and lends Riverkeeper engineers. Aster owes 12 Food at Epoch close.",
      mandateTitle: "Make the Oath Hold",
      mandateCopy: "Repair the East Sluice before sunset. Mara won access by promising twelve crates after harvest.",
      inherited: "MR-031-OATH · Food debt 12 · Service stair open · Riverkeepers present",
      dialogue: "Mara spoke for Aster. Twelve crates after harvest. I opened the service stair for you, Maker."
    },
    charter: {
      title: "Invoke the Founding Charter",
      quote: "No grain was promised.",
      summary: "The Charter compels access. Tala complies, locks the service stair, and withdraws every Riverkeeper engineer.",
      mandateTitle: "Repair What the Charter Took",
      mandateCopy: "Restore water without Riverkeeper help—or reopen the relationship Mara closed.",
      inherited: "MR-031-CHARTER · No Food debt · Service stair locked · Riverkeepers withdrawn",
      dialogue: "Your envoy brought law, not trust. The city may have its water. It will not have my help."
    }
  };

  const resolutions = {
    service: {
      code: "R31-1",
      title: "Shared hands raised the gate.",
      body: "Ivo and the Riverkeepers repaired the inner gate together. Water reaches the eastern cistern; Mara's Food debt remains part of Aster's future.",
      status: "stabilized",
      memory: "Tala remembers that Aster listened before asking her people to work.",
      next: [
        ["M-032", "MAKER", "Prepare Twelve Crates"],
        ["E-019", "ENVOY", "Write River Compact"],
        ["S-044", "SEEKER", "Inspect Flooded Archive"]
      ]
    },
    millrace: {
      code: "R31-2",
      title: "Old stone carries new water.",
      body: "Ivo reopened the old millrace. The sluice is stable and the oath still stands, but Tala remembers that Aster bypassed her engineers.",
      status: "stabilized",
      memory: "Tala remembers both the grain oath and the decision to bypass her stair.",
      next: [
        ["E-020", "ENVOY", "Answer for the Bypass"],
        ["M-032", "MAKER", "Prepare Twelve Crates"],
        ["S-045", "SEEKER", "Survey Old Millrace"]
      ]
    },
    cut: {
      code: "R31-3",
      title: "Water returns. A new fracture opens.",
      body: "Ivo cut through the sealed millrace alone. The eastern cistern fills, but the Riverkeepers walk out and Cohesion falls to 41.",
      status: "stabilized_with_fracture",
      memory: "Tala remembers that Aster used the Charter, then cut around the people who knew the river.",
      next: [
        ["E-021", "ENVOY", "Stop Riverkeeper Walkout"],
        ["W-014", "WARDEN", "Hold Eastern Cistern"],
        ["S-046", "SEEKER", "Record Charter Cut"]
      ]
    },
    reconcile: {
      code: "R31-4",
      title: "Terms reopen with the service stair.",
      body: "Ivo returns the Charter and negotiates a smaller obligation. Tala reopens the stair; Water returns and trust begins to recover.",
      status: "stabilized",
      memory: "Tala remembers that a Maker repaired more than stone.",
      next: [
        ["M-033", "MAKER", "Prepare Eight Crates"],
        ["E-022", "ENVOY", "Ratify New Water Terms"],
        ["S-047", "SEEKER", "Map Reopened Stair"]
      ]
    }
  };

  const nodes = {
    capital: {
      kicker: "CAPITAL · SHARED CIVILIZATION",
      title: "Aster Central",
      copy: "No player owns Aster. Forty active citizens inherit the same resources, laws, memories, and consequences.",
      note: "Mandates are individual. Civilization state is shared.",
      tone: "teal"
    },
    guild: {
      kicker: "INDEPENDENT SOCIETY",
      title: "River Guild",
      copy: "The Riverkeepers maintain knowledge Aster needs but does not own. Their cooperation is a relationship, not a resource meter.",
      note: "Tala remembers promises, refusals, and the way each was carried out.",
      tone: ""
    },
    farms: {
      kicker: "FOOD WORKSITE",
      title: "Northern Terraces",
      copy: "The harvest is healthy, but every civic promise competes with winter reserve. Mara can create a debt here without spending Food immediately.",
      note: "Current yield +2 · Reserve 46 · Outstanding debt updates with the world.",
      tone: "teal"
    },
    coast: {
      kicker: "TRADE DISTRICT",
      title: "Glass Harbor",
      copy: "Three caravans arrived this bell. Their fees flow into the shared Season Purse; they do not settle it.",
      note: "The purse continues accumulating until the published season outcome is finalized.",
      tone: ""
    }
  };

  const app = document.getElementById("app");
  const drawer = document.getElementById("context-drawer");
  const drawerContent = document.getElementById("drawer-content");
  const resetDialog = document.getElementById("reset-dialog");
  const identityOverlay = document.getElementById("identity-overlay");
  let state = loadState();
  app.classList.toggle("judge-mode", judgeMode);

  function seedA(target, choice) {
    Object.assign(target, choice === "oath"
      ? {
          aChoice: "oath", water: 36, cohesion: 60, foodDebt: 12, talaTrust: 20,
          serviceStair: "open", memoryReceipt: "MR-031-OATH", worksiteStatus: "access_resolved"
        }
      : {
          aChoice: "charter", water: 33, cohesion: 46, foodDebt: 0, talaTrust: -20,
          serviceStair: "locked", memoryReceipt: "MR-031-CHARTER", worksiteStatus: "access_resolved"
        });
  }

  function seedB(target, choice) {
    const outcome = resolutions[choice];
    const values = {
      service: { water: 60, cohesion: 64, timber: 12, prosperity: 43 },
      millrace: { water: 54, cohesion: 56, timber: 15, prosperity: 41 },
      cut: { water: 55, cohesion: 41, timber: 8, prosperity: 39 },
      reconcile: {
        water: 51, cohesion: 54, timber: 12, prosperity: 42,
        foodDebt: 8, talaTrust: 5, serviceStair: "open"
      }
    }[choice];
    Object.assign(target, values, {
      bChoice: choice,
      worksiteStatus: outcome.status,
      worksiteResolution: outcome.code,
      nextMandates: outcome.next.map(([id, role, title]) => ({ id, role, title }))
    });
  }

  function previewState(code) {
    const seeded = initialState();
    const a = (choice) => seedA(seeded, choice);
    const b = (choice) => seedB(seeded, choice);
    if (code === "scene-a") Object.assign(seeded, { view: "scene-a", flow: "scene_a" });
    if (code === "after-a-oath") {
      a("oath");
      Object.assign(seeded, { view: "access-resolution", flow: "handoff" });
    }
    if (code === "scene-b-oath") {
      a("oath");
      Object.assign(seeded, { currentCitizen: "ivo", view: "scene-b", flow: "scene_b" });
    }
    if (code === "resolved-r31-1" || code === "chronicle-r31-1") {
      a("oath");
      b("service");
      Object.assign(seeded, {
        currentCitizen: "ivo", flow: "resolved",
        view: code.startsWith("chronicle") ? "chronicle" : "worksite-resolution"
      });
    }
    if (code === "resolved-r31-3") {
      a("charter");
      b("cut");
      Object.assign(seeded, { currentCitizen: "ivo", flow: "resolved", view: "worksite-resolution" });
    }
    if (code === "vault") seeded.view = "vault";
    return seeded;
  }

  function loadState() {
    if (previewCode) return previewState(previewCode);
    try {
      const saved = JSON.parse(localStorage.getItem(STORAGE_KEY));
      if (!saved || typeof saved !== "object") return initialState();
      return { ...initialState(), ...saved, pendingChoice: null, drawerOpen: true };
    } catch {
      return initialState();
    }
  }

  function persist() {
    state.updatedAt = Date.now();
    if (!previewCode) localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  }

  function setView(view, open = true) {
    state.view = view;
    state.drawerOpen = open;
    state.pendingChoice = null;
    persist();
    render();
    if (open) drawer.scrollTo({ top: 0, behavior: "smooth" });
  }

  function openNode(node) {
    if (node === "sluice") {
      setView(state.worksiteResolution ? "worksite-resolution" : "pulse");
      return;
    }
    setView(`node-${node}`);
  }

  function metric(label, value, delta = "") {
    const warning = value < 50;
    return `<div class="drawer-metric">
      <header><span>${label}</span><strong>${value}${delta ? `<em class="${delta.startsWith("−") ? "bad" : ""}">${delta}</em>` : ""}</strong></header>
      <div class="metric-line ${warning ? "warning" : ""}"><i style="width:${Math.max(0, Math.min(100, value))}%"></i></div>
    </div>`;
  }

  function ruleBoundary() {
    return `<div class="rule-boundary" aria-label="Prototype responsibility boundary">
      <span><i class="ai"></i>AI: DIALOGUE</span>
      <span><i class="rules"></i>RULES: OUTCOME</span>
      <span><i class="chain"></i>SOLANA: CANON*</span>
    </div>`;
  }

  function seasonRule() {
    return `<div class="world-rule"><i></i><span>This resolves one Worksite, not the season. Aster continues from the changed state.</span></div>`;
  }

  function renderPulse() {
    if (state.worksiteResolution) {
      const result = resolutions[state.bChoice];
      return `<section>
        <div class="drawer-hero teal">
          <span class="kicker">SEASON ZERO · EPOCH ${state.epoch}/${state.epochCount} · LIVE</span>
          <h1>Aster continues.</h1>
          <p>Worksite 31 changed the shared world. It did not end it.</p>
        </div>
        <div class="drawer-body">
          <div class="resolution-banner ${result.status.includes("fracture") ? "fracture" : ""}">
            <small>${result.code} · EAST SLUICE · ${result.status.replaceAll("_", " ").toUpperCase()}</small>
            <strong>${result.title}</strong>
            <p>${result.body}</p>
          </div>
          <div class="section-label"><span>SHARED STATE NOW</span><strong>WORLD SYNC</strong></div>
          <div class="metric-list">${metric("Water", state.water)}${metric("Cohesion", state.cohesion)}${metric("Prosperity Path", state.prosperity)}</div>
          <div class="button-row"><button class="primary-button" data-action="view-next-mandates">SEE NEXT MANDATES</button><button class="ghost-button" data-action="view-chronicle">TRACE CAUSE</button></div>
          ${seasonRule()}
        </div>
      </section>`;
    }
    if (state.aChoice) {
      const branch = branchCopy[state.aChoice];
      return `<section>
        <div class="drawer-hero ${state.aChoice === "charter" ? "coral" : "teal"}">
          <span class="kicker">WORLD STATE CHANGED · ${state.memoryReceipt}</span>
          <h1>${state.aChoice === "oath" ? "The gate is open. The promise remains." : "The gate is open. The relationship is closed."}</h1>
          <p>Mara's action is now Ivo's starting world.</p>
        </div>
        <div class="drawer-body">
          <div class="memory-card"><small>INHERITED CIVIC MEMORY</small><strong>“${branch.quote}”</strong><p>${branch.summary}</p></div>
          <div class="section-label"><span>NEXT REQUIRED CITIZEN</span><strong>IVO · MAKER</strong></div>
          <div class="alert-card"><small>MANDATE 31-B · GENERATED FROM MARA'S RESULT</small><strong>${branch.mandateTitle}</strong><p>${branch.mandateCopy}</p></div>
          <div class="button-row"><button class="primary-button" data-action="switch-ivo">CONTINUE AS IVO</button></div>
          ${seasonRule()}
        </div>
      </section>`;
    }
    return `<section>
      <div class="drawer-hero coral">
        <span class="kicker">CIVILIZATION PULSE · EPOCH ${state.epoch}/${state.epochCount}</span>
        <h1>The river remembers what Aster owes.</h1>
        <p>Twenty-seven years ago, Aster took the river and failed to repay it.</p>
      </div>
      <div class="drawer-body">
        <div class="alert-card"><small>ACTIVE CIVIC CRISIS · WORKSITE 31</small><strong>The East Sluice closes at sunset.</strong><p>At the next bell, 4,800 citizens lose water. Tala will speak to one envoy.</p></div>
        <div class="section-label"><span>CIVILIZATION CONDITION</span><strong>ONE SHARED STATE</strong></div>
        <div class="metric-list">${metric("Water", state.water, "−4")}${metric("Food", state.food, "+2")}${metric("Cohesion", state.cohesion, "±0")}</div>
        <div class="button-row"><button class="primary-button" data-action="open-mandate-a">OPEN MARA'S MANDATE</button></div>
        <div class="world-rule"><i></i><span>Your individual action becomes the next citizen's shared starting condition.</span></div>
      </div>
    </section>`;
  }

  function renderNode(nodeKey) {
    const node = nodes[nodeKey] || nodes.capital;
    const dynamic = nodeKey === "guild" && state.aChoice
      ? state.aChoice === "oath"
        ? "Mara's oath moved Riverkeeper engineers into the sluice. The relationship now carries a twelve-crate obligation."
        : "Mara's Charter claim compelled access. Tala withdrew her engineers and locked the service stair."
      : node.note;
    return `<section>
      <div class="drawer-hero ${node.tone}"><span class="kicker">${node.kicker}</span><h1>${node.title}</h1><p>${node.copy}</p></div>
      <div class="drawer-body">
        <div class="info-card"><small>WHY THIS PLACE MATTERS</small><strong>${dynamic}</strong></div>
        <div class="section-label"><span>CIVIC LAYER</span><strong>SHARED WORLD</strong></div>
        <div class="metric-list">${metric("Water", state.water)}${metric("Food", state.food)}${metric("Cohesion", state.cohesion)}</div>
        <div class="button-row"><button class="primary-button" data-action="return-pulse">OPEN CIVILIZATION PULSE</button></div>
      </div>
    </section>`;
  }

  function mandateItem(id, role, title, detail, action = "", status = "OPEN") {
    return `<div class="mandate-item ${action ? "open" : ""}">
      <span>${role[0]}</span><div><strong>${title}</strong><small>${id} · ${role} · ${detail}</small></div>
      ${action ? `<button class="ghost-button" data-action="${action}">OPEN</button>` : `<em>${status}</em>`}
    </div>`;
  }

  function renderMandates() {
    let content;
    if (state.worksiteResolution) {
      content = state.nextMandates.map((m) => mandateItem(m.id, m.role, m.title, "Generated from Worksite 31", "", "QUEUED")).join("");
    } else if (state.aChoice) {
      content = mandateItem("31-B", "MAKER", branchCopy[state.aChoice].mandateTitle, "Memory-altered by Mara", "open-mandate-b");
    } else {
      content = mandateItem("31-A", "ENVOY", "Open the East Sluice", "Tala requested one envoy", "open-mandate-a");
    }
    return `<section>
      <div class="drawer-hero"><span class="kicker">ROLE-BASED CIVIC WORK</span><h1>${state.worksiteResolution ? "The world generated new work." : "Mandates begin with the world."}</h1><p>Citizens do not follow one hero quest. They receive bounded work from a shared civilization state.</p></div>
      <div class="drawer-body">
        <div class="section-label"><span>${state.worksiteResolution ? "NEXT MANDATES" : "ACTIVE MANDATE"}</span><strong>${state.worksiteResolution ? "3 GENERATED" : "1 OPEN"}</strong></div>
        <div class="mandate-list">${content}</div>
        ${state.worksiteResolution ? `<div class="active-season-note">Season Zero remains active. These Mandates are consequences of ${state.worksiteResolution}, not post-game content.</div>` : ""}
        ${seasonRule()}
      </div>
    </section>`;
  }

  function objectiveRow(index, title, detail, stateLabel = "OPEN") {
    return `<div class="objective-row"><span>${index}</span><div><strong>${title}</strong><small>${detail}</small></div><em>${stateLabel}</em></div>`;
  }

  function renderMandateA() {
    return `<section>
      <div class="drawer-hero coral"><span class="kicker">MANDATE 31-A · ENVOY · MARA VENN</span><h1>Open the East Sluice</h1><p>Secure access before sunset without pretending the past did not happen.</p></div>
      <div class="drawer-body">
        <div class="info-card"><small>WHY MARA</small><strong>Tala asked for one envoy, not an army.</strong><p>Mara carries Aster's word. She does not carry a right to command Tala's people.</p></div>
        <div class="section-label"><span>OBJECTIVES</span><strong>FIXED RULES</strong></div>
        <div class="objectives">${objectiveRow("01", "Gain water access", "Raise Water above 32")}${objectiveRow("02", "Keep civic order", "Leave Cohesion above 45")}${objectiveRow("03", "Create history", "Write one verifiable memory")}</div>
        <div class="button-row"><button class="primary-button" data-action="start-scene-a">SPEAK WITH TALA</button></div>
        ${ruleBoundary()}
      </div>
    </section>`;
  }

  function renderSceneA() {
    const selected = state.pendingChoice;
    return `<section class="scene-panel scene-a-panel">
      <div class="drawer-hero coral"><span class="kicker">EAST SLUICE · LIVE NPC ENCOUNTER</span><h1>Access is a social problem.</h1><p>The gate can open two ways. The world after it will not be the same.</p></div>
      <div class="drawer-body">
        <div class="npc-header"><img src="assets/tala-pixel-v1.png" alt="Portrait of Tala, Riverkeeper elder" /><div><small>RIVERKEEPER ELDER · REMEMBERS YOUR CHOICES</small><h2>Tala</h2><p>Her words are alive; Aster's civic rules remain fixed.</p></div></div>
        <div class="npc-dialogue">Aster remembers the Charter. Does Aster remember the grain it promised my mother?</div>
        <div class="section-label"><span>MARA'S RESPONSE</span><strong>CHOOSE ONE</strong></div>
        <div class="choice-stack">
          <button class="choice-card ${selected === "oath" ? "selected" : ""}" data-choice-a="oath"><span class="choice-letter">A</span><div><strong>Bind the Grain Oath</strong><p>Promise twelve crates after harvest in exchange for engineers and the service stair.</p><div class="effect-pills"><span class="good">WATER +8</span><span class="good">COHESION +6</span><span class="bad">FOOD DEBT 12</span><span class="future">FUTURE MEMORY</span></div></div></button>
          <button class="choice-card ${selected === "charter" ? "selected" : ""}" data-choice-a="charter"><span class="choice-letter">B</span><div><strong>Invoke the Founding Charter</strong><p>Compel the gate to open without promising resources.</p><div class="effect-pills"><span class="good">WATER +5</span><span class="bad">COHESION −8</span><span>NO DEBT</span><span class="future">ENGINEERS LEAVE</span></div></div></button>
        </div>
        ${ruleBoundary()}
        <div class="button-row"><button class="primary-button" data-action="commit-a" ${selected ? "" : "disabled"}>COMMIT TO SHARED WORLD</button></div>
      </div>
    </section>`;
  }

  function renderAccessResolution() {
    const branch = branchCopy[state.aChoice];
    const oath = state.aChoice === "oath";
    return `<section class="access-resolution-panel">
      <div class="drawer-hero ${oath ? "teal" : "coral"}"><span class="kicker">ACCESS RESOLVED · WORKSITE STILL ACTIVE</span><h1>${oath ? "The river moves. The debt remains." : "The gate opens. The Riverkeepers leave."}</h1><p>The shared world has changed; Worksite 31 still needs a Maker.</p></div>
      <div class="drawer-body">
        <div class="receipt"><header><span>MEMORY RECEIPT</span><strong>${state.memoryReceipt}</strong></header><blockquote>“${branch.quote}”</blockquote><dl><dt>ACTOR</dt><dd>Mara Venn · Envoy</dd><dt>LOCATION</dt><dd>East Sluice · Worksite 31</dd><dt>RULE RESULT</dt><dd>${branch.summary}</dd><dt>STATUS</dt><dd>Local prototype · planned Solana canon</dd></dl></div>
        <div class="section-label"><span>STATE DELTA</span><strong>SHARED BY ALL CITIZENS</strong></div>
        <div class="delta-grid"><div class="delta-card"><small>WATER</small><strong>${state.water}</strong><em>+${oath ? 8 : 5}</em></div><div class="delta-card"><small>COHESION</small><strong>${state.cohesion}</strong><em class="${oath ? "" : "bad"}">${oath ? "+6" : "−8"}</em></div><div class="delta-card"><small>FOOD DEBT</small><strong>${state.foodDebt}</strong><em class="${oath ? "bad" : ""}">${oath ? "OWED" : "NONE"}</em></div><div class="delta-card"><small>SERVICE STAIR</small><strong>${state.serviceStair.toUpperCase()}</strong><em>${oath ? "ACCESS" : "LOCKED"}</em></div></div>
        <div class="button-row"><button class="primary-button" data-action="switch-ivo">HAND WORLD TO IVO</button></div>
        ${seasonRule()}
      </div>
    </section>`;
  }

  function renderMandateB() {
    const branch = branchCopy[state.aChoice];
    return `<section>
      <div class="drawer-hero ${state.aChoice === "oath" ? "teal" : "coral"}"><span class="kicker">MANDATE 31-B · MAKER · MEMORY-ALTERED</span><h1>${branch.mandateTitle}</h1><p>${branch.mandateCopy}</p></div>
      <div class="drawer-body">
        <div class="memory-card"><small>INHERITED FROM MARA</small><strong>${branch.inherited}</strong><p>Ivo cannot replay Mara's scene. He begins inside its result.</p></div>
        <div class="section-label"><span>OBJECTIVES</span><strong>WORKSITE 31</strong></div>
        <div class="objectives">${objectiveRow("01", "Restore the flow", "Raise Water to 50 or higher")}${objectiveRow("02", "Read the inherited world", "Use or answer Mara's civic memory")}${objectiveRow("03", "Create downstream work", "Resolve this Worksite and generate new Mandates")}</div>
        <div class="button-row"><button class="primary-button" data-action="start-scene-b">ENTER THE SLUICE</button></div>
        ${ruleBoundary()}
      </div>
    </section>`;
  }

  function renderSceneB() {
    const oath = state.aChoice === "oath";
    const selected = state.pendingChoice;
    const choices = oath
      ? [
          ["service", "A", "Repair through the service stair", "Work beside the Riverkeepers and honor the relationship Mara created.", ["WATER +24", "COHESION +4", "TIMBER −6", "DEBT PERSISTS"]],
          ["millrace", "B", "Reopen the old millrace", "Bypass the stair. Save material, but let Tala remember the bypass.", ["WATER +18", "COHESION −4", "TIMBER −3", "NEW MEMORY"]]
        ]
      : [
          ["cut", "A", "Cut through the sealed millrace", "Restore water alone. Fast, expensive, and socially corrosive.", ["WATER +22", "COHESION −5", "TIMBER −10", "NEW FRACTURE"]],
          ["reconcile", "B", "Return the Charter and renegotiate", "Reopen terms with Tala, accept a smaller debt, and repair together.", ["WATER +18", "COHESION +8", "FOOD DEBT 8", "TRUST RECOVERS"]]
        ];
    const affordance = oath
      ? `<strong>MARA PROMISED 12 FOOD</strong><i>→</i><strong>TALA KEPT HER ENGINEERS</strong><i>→</i><strong>SERVICE STAIR UNLOCKED FOR YOU</strong><small>WITHOUT THIS OATH: SERVICE STAIR LOCKED · RIVERKEEPERS ABSENT</small>`
      : `<strong>MARA INVOKED THE CHARTER</strong><i>→</i><strong>TALA WITHDREW ENGINEERS</strong><i>→</i><strong>CUT / RECONCILE ONLY</strong><small>WITH THE OATH: SERVICE STAIR REPAIR · RIVERKEEPERS PRESENT</small>`;
    return `<section class="scene-panel scene-b-panel">
      <div class="drawer-hero ${oath ? "teal" : "coral"}"><span class="kicker">EAST SLUICE · SECOND CITIZEN · SAME WORLD</span><h1>Repair the gate—or the relationship.</h1><p>Mara's consequence has become Ivo's available action space.</p></div>
      <div class="drawer-body">
        <div class="affordance-ribbon ${oath ? "" : "fracture"}">${affordance}</div>
        <div class="npc-header"><img src="assets/tala-pixel-v1.png" alt="Portrait of Tala, Riverkeeper elder" /><div><small>TALA REMEMBERS · ${state.memoryReceipt}</small><h2>Tala</h2><p>Your earlier civic promise now changes what she says—and what Ivo can do.</p></div></div>
        <div class="npc-dialogue">${branchCopy[state.aChoice].dialogue}</div>
        <div class="memory-card"><small>CAUSAL INPUT FROM MARA</small><strong>“${branchCopy[state.aChoice].quote}”</strong><p>${branchCopy[state.aChoice].inherited}</p></div>
        <div class="section-label"><span>IVO'S ACTION</span><strong>CHOOSE ONE</strong></div>
        <div class="choice-stack">${choices.map(([id, letter, title, copy, effects]) => `<button class="choice-card ${selected === id ? "selected" : ""}" data-choice-b="${id}"><span class="choice-letter">${letter}</span><div><strong>${title}</strong><p>${copy}</p><div class="effect-pills">${effects.map((effect, index) => `<span class="${effect.includes("−") || effect.includes("FRACTURE") ? "bad" : index < 2 ? "good" : "future"}">${effect}</span>`).join("")}</div></div></button>`).join("")}</div>
        ${ruleBoundary()}
        <div class="button-row"><button class="primary-button" data-action="commit-b" ${selected ? "" : "disabled"}>RESOLVE THIS WORKSITE</button></div>
      </div>
    </section>`;
  }

  function renderWorksiteResolution() {
    const result = resolutions[state.bChoice];
    const fracture = result.status.includes("fracture");
    const before = state.aChoice === "oath" ? { water: 36, cohesion: 60, timber: 18 } : { water: 33, cohesion: 46, timber: 18 };
    return `<section class="worksite-resolution-panel">
      <div class="drawer-hero ${fracture ? "coral" : "teal"}"><span class="kicker">WORKSITE 31 RESOLVED · SEASON CONTINUES</span><h1>${result.title}</h1><p>${result.body}</p></div>
      <div class="drawer-body">
        <div class="resolution-banner ${fracture ? "fracture" : ""}"><small>${result.code} · ${result.status.replaceAll("_", " ").toUpperCase()}</small><strong>Local result recorded.</strong><p>The season remains active; its outcome and purse settlement are still unresolved.</p></div>
        <div class="result-delta-strip"><div><span>WATER</span><strong>${before.water} → ${state.water}</strong></div><div><span>COHESION</span><strong>${before.cohesion} → ${state.cohesion}</strong></div><div><span>TIMBER</span><strong>${before.timber} → ${state.timber}</strong></div><div><span>SEASON</span><strong>ACTIVE</strong></div></div>
        <div class="section-label"><span>NEXT MANDATES</span><strong>WORLD GENERATED</strong></div>
        <div class="mandate-list">${state.nextMandates.map((m) => mandateItem(m.id, m.role, m.title, `Caused by ${result.code}`, "", "QUEUED")).join("")}</div>
        <div class="causal-proof-detail"><div class="section-label"><span>CAUSAL PROOF</span><strong>2 CITIZENS · 1 WORLD</strong></div>
        <div class="causal-chain"><div class="causal-step"><span>01</span><div><strong>Mara changed access</strong><p>${branchCopy[state.aChoice].summary}</p></div></div><div class="causal-step"><span>02</span><div><strong>Ivo inherited the result</strong><p>His repair options were changed by ${state.memoryReceipt}.</p></div></div><div class="causal-step"><span>03</span><div><strong>${result.code} changed Aster</strong><p>${result.memory}</p></div></div><div class="causal-step"><span>04</span><div><strong>Three new Mandates appeared</strong><p>The changed world now asks different citizens to continue it.</p></div></div></div></div>
        <div class="button-row"><button class="primary-button" data-action="return-pulse">RETURN TO LIVING WORLD</button><button class="ghost-button" data-action="view-chronicle">OPEN CHRONICLE</button></div>
        ${seasonRule()}
      </div>
    </section>`;
  }

  function renderCitizens() {
    const completed = Boolean(state.worksiteResolution);
    return `<section>
      <div class="drawer-hero"><span class="kicker">40 ACTIVE CITIZENS · ONE CIVILIZATION</span><h1>Different hands, continuous history.</h1><p>Roles limit what each citizen can do. Shared state preserves what everyone did.</p></div>
      <div class="drawer-body">
        <div class="section-label"><span>FEATURED CITIZENS</span><strong>LOCAL DEMO</strong></div>
        <div class="citizen-list"><div class="citizen-item"><span>M</span><div><strong>Mara Venn</strong><small>Envoy · Mandate 31-A · ${state.aChoice ? "World state written" : "At East Sluice"}</small></div><em>${state.currentCitizen === "mara" ? "ACTIVE" : state.aChoice ? "DONE" : "READY"}</em></div><div class="citizen-item"><span>I</span><div><strong>Ivo Sen</strong><small>Maker · Mandate 31-B · ${state.aChoice ? completed ? "Worksite resolved" : "Inherited Mara's result" : "Waiting for world change"}</small></div><em>${state.currentCitizen === "ivo" ? completed ? "DONE" : "ACTIVE" : "WAITING"}</em></div><div class="citizen-item"><span>+</span><div><strong>38 simulated citizens</strong><small>Wardens, Seekers, Makers, Envoys · elsewhere in Aster</small></div><em>WORLD SYNC</em></div></div>
        <div class="active-season-note">This prototype follows two citizens deeply. The other 38 are simulated to show the intended shared-world scale, not claimed as live players.</div>
      </div>
    </section>`;
  }

  function ledgerItem(time, id, title, detail) {
    return `<div class="ledger-item"><time>${time}</time><div><strong>${title}</strong><small>${detail}</small></div><em>${id}</em></div>`;
  }

  function renderChronicle() {
    const result = state.worksiteResolution ? resolutions[state.bChoice] : null;
    return `<section>
      <div class="drawer-hero"><span class="kicker">LIVE CIVIC CHRONICLE · APPEND-ONLY</span><h1>History is still being written.</h1><p>The Chronicle explains who changed what while the season remains active.</p></div>
      <div class="drawer-body">
        <div class="section-label"><span>EPOCH ${state.epoch} · THE WATER DEBT</span><strong>LIVE</strong></div>
        <div class="ledger-list">${ledgerItem("B−12", "W31", "East Sluice reported dry", "Water 28 · 4,800 citizens at risk")}${state.aChoice ? ledgerItem("B−06", state.memoryReceipt, `Mara · ${branchCopy[state.aChoice].title}`, branchCopy[state.aChoice].summary) : ""}${result ? ledgerItem("NOW", result.code, `Ivo · ${result.title}`, result.body) : ""}${result ? ledgerItem("NOW", "GEN-3", "Three downstream Mandates generated", state.nextMandates.map((m) => m.id).join(" · ")) : ""}</div>
        ${state.aChoice ? `<div class="section-label"><span>CAUSE → CONSEQUENCE</span><strong>${result ? result.code : "WORKSITE ACTIVE"}</strong></div><div class="causal-chain"><div class="causal-step"><span>A</span><div><strong>Mara writes ${state.memoryReceipt}</strong><p>${branchCopy[state.aChoice].quote}</p></div></div><div class="causal-step"><span>B</span><div><strong>Ivo inherits changed affordances</strong><p>${branchCopy[state.aChoice].inherited}</p></div></div>${result ? `<div class="causal-step"><span>C</span><div><strong>${result.code} enters the live Chronicle</strong><p>${result.memory}</p></div></div>` : ""}</div>` : ""}
        <div class="active-season-note">Season outcome: unresolved · Purse: accumulating · Settlement: not started.</div>
        ${seasonRule()}
      </div>
    </section>`;
  }

  function renderVault() {
    return `<section>
      <div class="drawer-hero teal"><span class="kicker">SEASON PURSE · ACCUMULATING</span><h1>Fund the shared stakes, not every click.</h1><p>Entry fees, transactions, and marketplace fees can accumulate here during the season.</p></div>
      <div class="drawer-body">
        <div class="vault-amount"><small>SIMULATED SEASON PURSE</small><strong>348.50 <span>MOCK USDC</span></strong></div>
        <div class="vault-ledger"><div class="vault-row"><span>Season status</span><strong>ACTIVE · EPOCH ${state.epoch}/${state.epochCount}</strong></div><div class="vault-row"><span>Purse status</span><strong>ACCUMULATING</strong></div><div class="vault-row"><span>Season outcome</span><strong>UNRESOLVED</strong></div><div class="vault-row"><span>Settlement</span><strong>NOT STARTED</strong></div></div>
        <div class="section-label"><span>CIVILIZATION PATHS</span><strong>SEASON-LEVEL</strong></div>
        <div class="path-card"><header><strong>Prosperity</strong><span>${state.prosperity}%</span></header><div class="path-line"><i style="width:${state.prosperity}%"></i></div><p>Trade, supply, construction, and cohesion contribute over the whole season.</p></div>
        <div class="path-card"><header><strong>War</strong><span>22%</span></header><div class="path-line"><i style="width:22%"></i></div><p>Defense and territorial pressure remain below the current leading path.</p></div>
        <div class="active-season-note">Still accumulating. This Worksite changed civilization state, not season settlement. Claims open only after the published season outcome is finalized.</div>
        <div class="button-row"><button class="secondary-button" disabled>CLAIMS NOT OPEN</button></div>
      </div>
    </section>`;
  }

  function renderDrawer() {
    if (state.view === "pulse") return renderPulse();
    if (state.view.startsWith("node-")) return renderNode(state.view.slice(5));
    if (state.view === "mandates") return renderMandates();
    if (state.view === "mandate-a") return renderMandateA();
    if (state.view === "scene-a") return renderSceneA();
    if (state.view === "access-resolution") return renderAccessResolution();
    if (state.view === "mandate-b") return renderMandateB();
    if (state.view === "scene-b") return renderSceneB();
    if (state.view === "worksite-resolution") return renderWorksiteResolution();
    if (state.view === "citizens") return renderCitizens();
    if (state.view === "chronicle") return renderChronicle();
    if (state.view === "vault") return renderVault();
    return renderPulse();
  }

  function renderFeed() {
    let rows;
    if (state.worksiteResolution) {
      rows = [["NOW", state.worksiteResolution, resolutions[state.bChoice].title], ["01M", "GEN-3", "Three downstream Mandates entered the queue."], ["LIVE", "S0", "Season Zero remains active; purse keeps accumulating."]];
    } else if (state.aChoice) {
      rows = [["NOW", state.memoryReceipt, "Mara's civic memory changed Worksite 31."], ["01M", "31-B", `${branchCopy[state.aChoice].mandateTitle} generated for Ivo.`], ["12M", "SYNC", "40 citizens share the changed civilization state."]];
    } else {
      rows = [["NOW", "CRISIS", "Eastern cistern falls below one bell of reserve."], ["04M", "TALA", "Riverkeeper elder requests one envoy from Aster."], ["12M", "SYNC", "40 active citizens share this civilization state."]];
    }
    document.getElementById("world-feed-items").innerHTML = rows.map(([time, id, copy]) => `<div class="feed-row"><time>${time}</time><p>${copy}</p><em>${id}</em></div>`).join("");
  }

  function dockSection() {
    if (!state.drawerOpen) return "map";
    if (["mandates", "mandate-a", "scene-a", "access-resolution", "mandate-b", "scene-b", "worksite-resolution"].includes(state.view)) return "mandates";
    if (["citizens", "chronicle", "vault"].includes(state.view)) return state.view;
    return "map";
  }

  function renderChrome() {
    app.dataset.flow = state.flow;
    app.dataset.branch = state.aChoice || "none";
    app.dataset.resolution = state.worksiteResolution || "none";
    document.getElementById("water-value").textContent = state.water;
    document.getElementById("food-value").textContent = state.food;
    document.getElementById("cohesion-value").textContent = state.cohesion;
    document.getElementById("timber-value").textContent = state.timber;
    document.getElementById("prosperity-value").textContent = `${state.prosperity}%`;
    document.getElementById("water-delta").textContent = state.worksiteResolution ? "FLOW" : state.aChoice ? "+" : "−4";
    document.getElementById("cohesion-delta").textContent = state.cohesion < 50 ? "LOW" : state.aChoice ? "CHANGED" : "±0";
    const ivo = state.currentCitizen === "ivo";
    document.getElementById("citizen-monogram").textContent = ivo ? "I" : "M";
    document.getElementById("citizen-name").textContent = ivo ? "Ivo Sen" : "Mara Venn";
    document.getElementById("citizen-role").textContent = ivo ? "MAKER · 0871" : "ENVOY · 0214";
    document.getElementById("mandate-count").textContent = state.worksiteResolution ? "3" : "1";
    document.getElementById("chronicle-count").textContent = state.worksiteResolution ? "2" : state.aChoice ? "1" : "0";
    const resolved = Boolean(state.worksiteResolution);
    document.getElementById("map-status").textContent = resolved ? `EASTERN WATER NETWORK · ${state.worksiteResolution} RECORDED · SEASON ACTIVE` : state.aChoice ? "EASTERN WATER NETWORK · ACCESS CHANGED · WORKSITE ACTIVE" : "EASTERN WATER NETWORK · CRISIS ACTIVE";
    document.getElementById("guild-status").textContent = !state.aChoice ? "Awaiting envoy" : state.worksiteResolution ? state.bChoice === "service" ? "Working pact" : state.bChoice === "millrace" ? "Bypassed" : state.bChoice === "cut" ? "Walkout active" : "Terms reopened" : state.aChoice === "oath" ? "Engineers deployed" : "Engineers withdrawn";
    document.getElementById("sluice-status").textContent = resolved ? `${state.worksiteResolution} · Flow restored` : state.aChoice ? "Access resolved · Repair active" : "Closes at sunset";
    const sluiceNode = document.getElementById("sluice-node");
    sluiceNode.classList.toggle("resolved", resolved);
    const badge = sluiceNode.querySelector(":scope > b");
    if (badge) {
      badge.textContent = resolved ? "✓" : "1";
      badge.style.background = resolved ? "var(--cyan)" : "var(--coral)";
    }
    document.getElementById("sluice-path").classList.toggle("resolved", resolved);
    drawer.classList.toggle("open", state.drawerOpen);
    app.classList.toggle("drawer-collapsed", !state.drawerOpen);
    const eastViews = ["pulse", "mandate-a", "scene-a", "access-resolution", "mandate-b", "scene-b", "worksite-resolution"];
    app.classList.toggle("map-shift-east", state.drawerOpen && eastViews.includes(state.view));
    app.classList.toggle("story-scene-open", ["scene-a", "scene-b"].includes(state.view));
    const activeDock = dockSection();
    document.querySelectorAll(".dock-button[data-view]").forEach((button) => button.classList.toggle("active", button.dataset.view === activeDock));
    renderFeed();
  }

  function render() {
    drawerContent.innerHTML = renderDrawer();
    renderChrome();
  }

  function showToast(title, body) {
    const region = document.getElementById("toast-region");
    const item = document.createElement("div");
    item.className = "toast";
    item.innerHTML = `<strong>${title}</strong><span>${body}</span>`;
    region.appendChild(item);
    window.setTimeout(() => item.remove(), 3600);
  }

  function switchToIvo() {
    state.currentCitizen = "ivo";
    state.flow = "mandate_b";
    state.view = "mandate-b";
    state.drawerOpen = true;
    persist();
    identityOverlay.hidden = false;
    identityOverlay.innerHTML = `<div><small>CHANGING CITIZEN · NOT CHANGING WORLD</small><strong>Ivo Sen</strong><span>MAKER · inherits ${state.memoryReceipt}</span></div>`;
    window.setTimeout(() => {
      identityOverlay.hidden = true;
      render();
      drawer.scrollTo({ top: 0 });
    }, 1250);
  }

  function applyChoiceA() {
    if (!state.pendingChoice || !["oath", "charter"].includes(state.pendingChoice)) return;
    seedA(state, state.pendingChoice);
    state.pendingChoice = null;
    state.flow = "handoff";
    state.view = "access-resolution";
    persist();
    render();
    drawer.scrollTo({ top: 0, behavior: "smooth" });
    showToast("WORLD STATE UPDATED", `${state.memoryReceipt} now belongs to every citizen of Aster.`);
  }

  function applyChoiceB() {
    const valid = state.aChoice === "oath" ? ["service", "millrace"] : ["cut", "reconcile"];
    if (!valid.includes(state.pendingChoice)) return;
    seedB(state, state.pendingChoice);
    state.pendingChoice = null;
    state.flow = "resolved";
    state.view = "worksite-resolution";
    persist();
    render();
    drawer.scrollTo({ top: 0, behavior: "smooth" });
    showToast("WORKSITE 31 RESOLVED", `${state.worksiteResolution} entered the live Chronicle. Season Zero continues.`);
  }

  document.addEventListener("click", (event) => {
    const viewButton = event.target.closest("[data-view]");
    if (viewButton) {
      const view = viewButton.dataset.view;
      if (view === "map") setView(state.view, false);
      else setView(view);
      return;
    }
    const nodeButton = event.target.closest("[data-node]");
    if (nodeButton) {
      openNode(nodeButton.dataset.node);
      return;
    }
    const aChoice = event.target.closest("[data-choice-a]");
    if (aChoice) {
      state.pendingChoice = aChoice.dataset.choiceA;
      render();
      return;
    }
    const bChoice = event.target.closest("[data-choice-b]");
    if (bChoice) {
      state.pendingChoice = bChoice.dataset.choiceB;
      render();
      return;
    }
    const actionButton = event.target.closest("[data-action]");
    if (!actionButton) return;
    const action = actionButton.dataset.action;
    if (action === "open-mandate-a") setView("mandate-a");
    if (action === "start-scene-a") {
      state.flow = "scene_a";
      setView("scene-a");
    }
    if (action === "commit-a") applyChoiceA();
    if (action === "switch-ivo") switchToIvo();
    if (action === "open-mandate-b") setView("mandate-b");
    if (action === "start-scene-b") {
      state.flow = "scene_b";
      setView("scene-b");
    }
    if (action === "commit-b") applyChoiceB();
    if (action === "view-next-mandates") setView("mandates");
    if (action === "view-chronicle") setView("chronicle");
    if (action === "return-pulse") setView("pulse");
  });

  document.getElementById("drawer-close").addEventListener("click", () => setView(state.view, false));
  document.getElementById("reset-button").addEventListener("click", () => resetDialog.showModal());
  resetDialog.addEventListener("close", () => {
    if (resetDialog.returnValue !== "confirm") return;
    localStorage.removeItem(STORAGE_KEY);
    state = initialState();
    render();
    showToast("LOCAL WORLD RESET", "Aster returned to the first East Sluice crisis.");
  });

  render();
})();
