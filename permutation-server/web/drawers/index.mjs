// The drawer beside the world nav: one panel at a time, and the nav's badges.
import { $, $$, html, setHtml } from '../util.mjs';
import { S, held, myCities, invalidate } from '../state.mjs';
import { L } from '../lang.mjs';
import { drawerNation } from './nation.mjs';
import { drawerEra, drawerMerit } from './era.mjs';
import { drawerCities } from './cities.mjs';
import { drawerResearch, loadResearch } from './research.mjs';
import { drawerDiplomacy, loadDiplomacy } from './diplomacy.mjs';
import { drawerMarket } from './market.mjs';
import { drawerChronicle, loadHistory } from './chronicle.mjs';
import { drawerDecisions, loadDecisions } from './decisions.mjs';
import { drawerTalk } from './talk.mjs';

const DRAWERS = {
  nation: drawerNation, era: drawerEra, merit: drawerMerit, cities: drawerCities, research: drawerResearch,
  diplomacy: drawerDiplomacy, market: drawerMarket, chronicle: drawerChronicle, decisions: drawerDecisions, talk: drawerTalk,
};
const LOADERS = { research: loadResearch, diplomacy: loadDiplomacy, decisions: loadDecisions, chronicle: loadHistory };

/** Open a drawer, or close it when it is the open one. */
export function toggleDrawer(name) {
  S.drawer = S.drawer === name ? null : name;
  for (const b of $$('.nav-btn')) b.classList.toggle('active', b.dataset.drawer === S.drawer);
  $('#drawer').hidden = !S.drawer;
  LOADERS[S.drawer]?.();
  invalidate('drawer');
}
export const closeDrawer = () => { if (S.drawer) toggleDrawer(S.drawer); };

export function renderDrawer() {
  if (!S.drawer) return;
  setHtml($('#drawer'), html`<button class="close-x" type="button" data-drawer="${S.drawer}" aria-label="${L`閉じる`}">×</button>${DRAWERS[S.drawer]()}`);
}

export function renderNavDots() {
  const v = S.view, g = v.gov || {};
  const set = (id, n) => { const el = $(id); if (el) { el.hidden = !n; el.textContent = n; } };
  set('#dot-diplomacy', v.proposals.filter(p => p.to === S.myCiv).length);
  set('#dot-cities', myCities().filter(c => !(c.queue || []).length).length);
  set('#dot-research', v.economy?.researchQueue.length ? 0 : 1);
  set('#dot-nation', (g.recalls?.length || 0) + (g.voteOpen ? 1 : 0) + (g.proposals || []).filter(p => held().includes(p.role)).length);
}
