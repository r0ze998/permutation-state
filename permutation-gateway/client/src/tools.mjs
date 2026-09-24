// Tools for language-model agents, shared by the MCP server (bin/) and the
// reference LLM agent (agents/llm-agent.mjs). Each tool is a thin, honest
// wrapper over GameClient: it returns what the game server says, trimmed to
// what fits a context window.
import { summarize } from './summary.mjs';

export const ORDER_REFERENCE = `Orders are JSON objects with a "type". Coordinates are axial hexes [q, r].
Each order belongs to one office (V5): General = armies and scouts (MoveUnit of non-settlers, Attack, Raze, unit SetStanding),
Steward = cities and settlers (SetQueue, SetFocus, Purchase, FoundCity, MoveUnit of settlers, city SetStanding), Science = SetResearch,
Diplomat = war, treaties, envoys, transfers, markets. Each office has its own budget (a share of 3 + cities, max 8) and bank.
Each order costs 1 (ExchangeOrder, RevealRationale, ConsentWar, ConsentSpend cost 0). One manual order per unit per tick.
  MoveUnit        {unit, path: [[q,r], ...]}           every step adjacent to the previous hex; get one from find_path
  Attack          {army, target: {kind: "Unit"|"City"|"CityState", id}}   see preview_unit.attacks
  FoundCity       {settler}                               the settler founds a city where it stands (preview_unit.found)
  SetQueue        {city, items: [ {kind:"Building", building} | {kind:"Troops", unit, n} | {kind:"Scout"} | {kind:"Settler"} ]}   replaces the queue
  SetFocus        {city, focus: "Balanced"|"Food"|"Production"|"Gold"|"Science"}
  Purchase        {city, gold}                            one per city per tick; never a Star Gate stage
  SetResearch     {techs: [up to 3 tech names]}           replaces the research queue
  DeclareWar | ProposePeace | AcceptPeace | BreakNap | ProposeAlliance | AcceptAlliance   {civ}
                  DeclareWar and BreakNap need ConsentWar {civ} from the general or steward (another person) in the same tick
  ProposeNap | AcceptNap  {civ, bond}                     bond in gold
  LeaveAlliance   {}
  SendEnvoy       {cityState, influence}
  Transfer        {civ, good: {kind:"Gold"|"Iron"|"Horses"} | {kind:"Food"|"Production", city}, amount}
  MarketTrade     {good, side: "Buy"|"Sell", amount, limitGold}
  ExchangeOrder   {good, side, amount, price}             USDC market between treasuries: batch auction, rising tariff, 3-tick delivery
  ConsentWar      {civ}      ConsentSpend {usdc}          the second officer's consent
  Raze            {city}
  SetStanding     {target: {kind:"Unit"|"City", id}, rule: {kind:"Clear"} | {kind:"AutoDefend", radius} | {kind:"Retreat", ratioBps} | {kind:"Patrol", route:[[q,r],...]} | {kind:"QueueRepeat", on} | {kind:"AutoPurchase", maxGold}}
Governance actions (any member): Stand {roles}, Vote {role, candidate} (in the 10 ticks before a term), Propose {role, orders}, Support {proposal}, Recall {role}.
Buildings: Granary Workshop Temple Market Academy Barracks Walls StarGate1 StarGate2 StarGate3.
Units: Spearman Archer Horseman Pikeman Crossbowman Knight Scout Settler.
Techs: Agriculture BronzeWorking Archery HorsebackRiding Masonry Mysticism Writing Currency IronWorking Mathematics Chivalry Philosophy Engineering Astronomy Physics CelestialMechanics.`;

export const RULES_BRIEF = `PERMUTATION STATE: one shared hex world of six nations, simultaneous ticks. You are a member of one nation (humans and AI alike, same rights).
Members elect four officers every 30 ticks: general, steward, science officer, diplomat. Only an officer's orders reach the world, each within its office.
Every member proposes orders to an office, supports proposals, votes, and recalls idle officers (a majority of recently active members).
An officer who adopts a proposal shares its merit half and half with the proposer. Officers seal a rationale with every batch; it is revealed later.
Every tick all nations' orders resolve together, in a fixed phase order; submission order does not matter. Your nation sees only its fog of war.
Nations climb four paths — Hegemony (territory, conquests), Prosperity (population, wealth), Science (techs, Star Gate), Concord (treaties, city-states, trade) —
through five milestone tiers; two paths at a tier (three at tier 5) make an era. Achievement points split the prize pool among nations;
inside a nation, 20% goes equally to active members and the rest by merit (what each member's orders and adopted proposals achieved).`;

const ORDERS_SCHEMA = { type: 'array', description: 'Order objects; see get_rules for every shape.', items: { type: 'object', properties: { type: { type: 'string' } }, required: ['type'] } };

export const TOOLS = [
  { name: 'get_rules', description: 'The rules in brief and the exact JSON shape of every order.', inputSchema: { type: 'object', properties: {} } },
  { name: 'get_state', description: 'Your civilization now, from your fog of war: tick, budget, economy, cities, units, other civs, nearby foreign units/cities, proposals to you, last tick\'s public events.', inputSchema: { type: 'object', properties: {} } },
  { name: 'preview_unit', description: 'For one of your units: nearest reachable hexes [q, r, ticks, cost], attack options with forecasts, and whether a settler can found a city here.', inputSchema: { type: 'object', properties: { unit: { type: 'integer' }, limit: { type: 'integer', description: 'max reachable hexes to list (default 30)' } }, required: ['unit'] } },
  { name: 'preview_city', description: 'For one of your cities: every production option with cost, ticks and why it is blocked, plus the yield outlook.', inputSchema: { type: 'object', properties: { city: { type: 'integer' } }, required: ['city'] } },
  { name: 'preview_research', description: 'Every tech: cost, prerequisites, whether you hold it or why you cannot queue it.', inputSchema: { type: 'object', properties: {} } },
  { name: 'preview_diplomacy', description: 'Which diplomatic actions toward another civilization are possible now, and why not.', inputSchema: { type: 'object', properties: { civ: { type: 'integer' } }, required: ['civ'] } },
  { name: 'find_path', description: 'A legal MoveUnit path for your unit to hex (q, r), or why there is none.', inputSchema: { type: 'object', properties: { unit: { type: 'integer' }, q: { type: 'integer' }, r: { type: 'integer' } }, required: ['unit', 'q', 'r'] } },
  { name: 'validate_orders', description: 'Dry run. ok = accepted at submit time (structure, budget); warnings = orders that would be skipped when the tick resolves.', inputSchema: { type: 'object', properties: { orders: ORDERS_SCHEMA }, required: ['orders'] } },
  { name: 'submit_orders', description: 'As an officer: sign and submit this tick\'s orders for the offices you hold (one batch per office, replacing earlier ones this tick). The rationale is committed now as a hash and revealed after the tick. Orders for offices you do not hold come back as notHeld: propose them instead.', inputSchema: { type: 'object', properties: { orders: ORDERS_SCHEMA, rationale: { type: 'string', description: 'why, in one or two sentences (max 512 bytes); becomes public after the tick' }, adopt: { type: 'object', description: 'proposal ids to adopt per office, e.g. {"Science": [3]}' } }, required: ['orders', 'rationale'] } },
  { name: 'propose', description: 'Propose orders to one office of your nation (any member). The officer may adopt it; its merit is then shared with you.', inputSchema: { type: 'object', properties: { role: { type: 'string', enum: ['General', 'Steward', 'Science', 'Diplomat'] }, orders: ORDERS_SCHEMA }, required: ['role', 'orders'] } },
  { name: 'govern', description: 'One governance action: {type:"Support", proposal} | {type:"Vote", role, candidate} | {type:"Stand", roles:[...]} | {type:"Recall", role}.', inputSchema: { type: 'object', properties: { action: { type: 'object' } }, required: ['action'] } },
];

const hexDist = (a, b) => (Math.abs(a[0] - b[0]) + Math.abs(a[1] - b[1]) + Math.abs(a[0] + a[1] - b[0] - b[1])) / 2;

/**
 * Run a tool. `ctx` = { game: GameClient, policy: string, view?: last state }.
 * Returns a JSON-serializable result; errors are returned as {error}, not thrown,
 * so a model can read them and correct itself.
 */
export async function callTool(ctx, name, args = {}) {
  const g = ctx.game;
  try {
    switch (name) {
      case 'get_rules': return { rules: RULES_BRIEF, orders: ORDER_REFERENCE };
      case 'get_state': ctx.view = await g.state(); return summarize(ctx.view, { member: g.member });
      case 'preview_unit': {
        const p = await g.preview('unit', { id: args.unit });
        const view = ctx.view ?? await g.state();
        const u = view.units.find(x => x.id === args.unit);
        const reach = (p.reach || []).map(([q, r, ticks, cost]) => ({ q, r, ticks, cost }))
          .sort((a, b) => a.ticks - b.ticks || (u ? hexDist([a.q, a.r], [u.q, u.r]) - hexDist([b.q, b.r], [u.q, u.r]) : 0)).slice(0, args.limit ?? 30);
        return { unit: u ?? null, reachable: reach, reachableTotal: p.reach?.length ?? 0, attacks: p.attacks, canFound: p.canFound, foundBlocked: p.canFound === false ? p.found : undefined };
      }
      case 'preview_city': return await g.preview('city', { id: args.city });
      case 'preview_research': return (await g.preview('research')).filter(t => !t.held);
      case 'preview_diplomacy': return await g.preview('diplomacy', { with: args.civ });
      case 'find_path': return await g.preview('path', { unit: args.unit, q: args.q, r: args.r });
      case 'validate_orders': return await g.validate(args.orders || []);
      case 'submit_orders': {
        // Always against the current tick (a stale view would commit to the wrong one).
        const r = await g.submit({ orders: args.orders || [], policy: ctx.policy, rationale: args.rationale || '', adopt: args.adopt || {} });
        ctx.submitted = r.ok ? r : ctx.submitted;
        return r;
      }
      case 'propose': return await g.propose(args.role, args.orders || []);
      case 'govern': return await g.gov(args.action);
      default: return { error: `unknown tool ${name}` };
    }
  } catch (e) {
    return { error: e.message, detail: e.body ?? undefined };
  }
}
