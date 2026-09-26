import { test } from 'node:test';
import assert from 'node:assert/strict';
import { blockedText, chronicleText, civName, cityName, phaseOf, standingText } from '../../permutation-server/web/i18n.mjs';

// [chronicle line, expected [kind, text]] — captured from the pre-refactor chain of
// if/match statements, so the table-driven chronicleText is pinned to identical output.
const CHRONICLE = [
  ["war|Aster declares war on Borealis",["war","アステルがボレアリスに宣戦"]],
  ["war|Aster declares war on Borealis, breaking a pact",["war","アステルがボレアリスに宣戦（条約破棄）"]],
  ["war|Aster declares war on Borealis with casus belli",["war","アステルがボレアリスに宣戦（正当な理由あり）"]],
  ["peace|Aster and Borealis make peace",["peace","アステルとボレアリスが講和"]],
  ["ally|Cinder and Dunmar form an alliance",["ally","シンダーとダンマールが同盟を結成"]],
  ["diplo|Ember and Fjordal sign a non-aggression pact",["diplo","エンバーとフィヨルダルが不可侵条約を締結"]],
  ["diplo|The pact between Aster and Cinder expires",["diplo","アステルとシンダーの不可侵条約が満了"]],
  ["ally|Borealis and Ember end their alliance",["ally","ボレアリスとエンバーの同盟が解消"]],
  ["found|Dunmar founds a new city",["found","ダンマールが新しい都市を建設"]],
  ["capture|Aster captures a city of Borealis",["capture","アステルがボレアリスの都市を占領"]],
  ["capture|Cinder captures a free city",["capture","シンダーが自由都市を占領"]],
  ["capture|Ember conquers a city-state",["capture","エンバーが都市国家を征服"]],
  ["revolt|A city of Fjordal revolts and becomes free",["revolt","フィヨルダルの都市が反乱し自由都市に"]],
  ["raze|A city is razed to a ruin",["raze","都市が破壊され遺跡になった"]],
  ["science|Aster completes Star Gate stage 2",["science","アステルがスターゲート第2段階を完成"]],
  ["diplo|Borealis becomes suzerain of city-state 3",["diplo","ボレアリスが都市国家3の宗主に"]],
  ["tech|Cinder discovers IronWorking",["tech","シンダーが「製鉄」を発見"]],
  ["tech|Cinder discovers Alchemy",["tech","シンダーが「Alchemy」を発見"]],
  ["gov|Aster: alice becomes general (was bob)",["gov","アステル：aliceが将軍に（前任 bob）"]],
  ["recall|Borealis: the acting official becomes science officer (was carol)",["recall","ボレアリス：代行が科学官に（解任による交代）（前任 carol）"]],
  ["gov|Cinder: dave becomes diplomat (was acting)",["gov","シンダー：daveが外交官に（前任 代行）"]],
  ["gov|First election — Dunmar: general alice, steward bob, science officer acting, diplomat the acting official",["gov","ダンマールの第1回選挙：将軍 alice・内政官 bob・科学官 代行・外交官 代行"]],
  ["gov|First election — Dunmar: general alice, mystery x",["gov","ダンマールの第1回選挙：将軍 alice・mystery x"]],
  ["gov|Ember adopts 1 proposal",["gov","エンバーが献策を1件採用"]],
  ["gov|Ember adopts 3 proposals",["gov","エンバーが献策を3件採用"]],
  ["milestone|Fjordal reaches Hegemony 2",["milestone","フィヨルダルが覇権の第2段階に到達"]],
  ["milestone|Fjordal loses Concord 1",["milestone","フィヨルダルが協調の第1段階を失った"]],
  ["era|Aster enters era 3",["era","アステルが第3時代に入った"]],
  ["bounty|Aster conquers the home of agent-7 of Borealis, an operator AI member: bounty 25 USDC",["bounty","アステルがボレアリスのagent-7（運営のAIメンバー）の住む都市を落とした：懸賞金 25 USDC"]],
  ["bounty|Cinder conquers the home of Helper Bot of Dunmar, an operator AI member: no bounty (treaty within 10 ticks)",["bounty","シンダーがダンマールのHelper Bot（運営のAIメンバー）の住む都市を落とした：直前に条約があったため懸賞金なし"]],
  ["other|Something unrecognised happens",["other","Something unrecognised happens"]],
  ["nokind",["nokind",""]],
  ["war|Zorg declares war on Aster",["war","Zorgがアステルに宣戦"]],
];

test('chronicleText: one sample per pattern (incl. bounty lines) matches the pinned output', () => {
  for (const [line, want] of CHRONICLE) assert.deepEqual(chronicleText(line), want, line);
});

test('chronicleText: an unknown path name falls back to the raw text', () => {
  assert.deepEqual(chronicleText('milestone|Aster reaches Glory 2'), ['milestone', 'アステルがGloryの第2段階に到達']);
  assert.deepEqual(chronicleText('milestone|Aster loses Glory 1'), ['milestone', 'アステルがGloryの第1段階を失った']);
});

test('i18n: small pure helpers', () => {
  assert.equal(civName('Aster'), 'アステル');
  assert.equal(civName('Zorg'), 'Zorg');
  assert.equal(cityName(0), 'ラナ');
  assert.equal(cityName(21), 'ヴェル 2');
  assert.deepEqual(phaseOf(0), [0, '草創', 'FOUNDING']);
  assert.deepEqual(phaseOf(130), [120, '危機', 'CRISIS']);
  assert.equal(blockedText(null), '');
  assert.equal(blockedText({ code: 'NotEnoughGold', need: 5, have: 2 }), '金が足りません（必要5・所持2）');
  assert.equal(blockedText({ code: 'SomethingNew' }), 'SomethingNew');
  assert.equal(standingText({ kind: 'Retreat', ratioBps: 15000 }), '撤退 · 相手が1.5倍を超えたら');
});
