// What the web client shows from the play server's views
// (permutation-server/web): the 'verify it yourself' command (util.mjs
// verifyCommand: an absolute gateway URL, RPC placeholders when none is
// public), and the operator's AI roster, which /api/state carries as
// `aiRoster` while `roster` is the nation's member list (state.mjs
// aiRoster/nationRoster, the talk drawer, the help text).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { verifyCommand, verifyNote } from '../../permutation-server/web/util.mjs';
import { S, aiRoster, nationRoster } from '../../permutation-server/web/state.mjs';
import { drawerTalk } from '../../permutation-server/web/drawers/talk.mjs';
import { othersText } from '../../permutation-server/web/lobby.mjs';

test('verify command: the gateway as an absolute URL, whatever the view gives', () => {
  // Behind the play server's proxy the view says "/gw" (relative to the page).
  const v = verifyCommand({ gateway: '/gw', endpoints: {} }, 'https://play.example.org/index.html?member=3');
  assert.equal(v.gateway, 'https://play.example.org/gw');
  assert.match(v.text, /--gateway https:\/\/play\.example\.org\/gw \\/);
  assert.doesNotMatch(v.text, /--gateway \/gw/);
  assert.equal(v.rpcKnown, false);
  assert.match(v.text, /--base <base RPC> --er <ER RPC>/, 'placeholders, never "undefined"');
  assert.doesNotMatch(v.text, /undefined|null/);
  assert.match(verifyNote(v), /RPC/);
  assert.match(verifyNote(v), /<base RPC>/);
  // A trailing slash goes; an absolute gateway stays as it is.
  assert.equal(verifyCommand({ gateway: '/gw/' }, 'http://127.0.0.1:47123/').gateway, 'http://127.0.0.1:47123/gw');
  assert.equal(verifyCommand({ gateway: 'http://10.0.0.2:4188/' }, 'https://play.example.org/').gateway, 'http://10.0.0.2:4188');
  // Public RPC URLs (the operator chose to publish them) are used, quoted for the shell when needed.
  const pub = verifyCommand({ gateway: '/gw', endpoints: { base: 'https://api.devnet.solana.com', er: 'https://devnet.magicblock.app/?a=1&b=2' } }, 'https://play.example.org/');
  assert.equal(pub.rpcKnown, true);
  assert.match(pub.text, /--base https:\/\/api\.devnet\.solana\.com --er 'https:\/\/devnet\.magicblock\.app\/\?a=1&b=2'$/);
  assert.doesNotMatch(verifyNote(pub), /<base RPC>/);
  // Only http(s) URLs are taken from the view.
  assert.match(verifyCommand({ gateway: '/gw', endpoints: { base: 'javascript:alert(1)', er: 'x' } }, 'https://a.example/').text, /<base RPC> --er <ER RPC>/);
  assert.equal(verifyCommand({}, 'https://a.example/'), null);
  assert.equal(verifyCommand(null), null);
});

// /api/state?member=M as the play server sends it: `roster` the nation's members, `aiRoster` the AI roster.
const fallen = { member: 7, name: 'Ilse', civ: 2, salt: 'ab'.repeat(32), home: 4, captor: 1, tick: 60, bounty: '5000000' };
const memberView = () => ({
  chain: { seasonId: '7' }, civs: [{ id: 0, name: 'Aster' }, { id: 1, name: 'Borealis' }, { id: 2, name: 'Cyrene' }],
  members: [{ id: 0, name: 'A', civ: 0 }, { id: 1, name: 'B', civ: 0 }, { id: 7, name: 'Ilse', civ: 2 }],
  member: { id: 0, name: 'A' }, talk: [],
  roster: [{ id: 0, name: 'A', kind: 'undeclared', merit: 1.5, active: true, activeWindows: 2 }, { id: 1, name: 'B', kind: 'undeclared', merit: 0, active: false, activeWindows: 0 }],
  aiRoster: { aiCount: 4, bountyEach: '5000000', homeTick: 45, revealed: false, fallen: [fallen] },
});

test('the AI roster comes from aiRoster; roster stays the nation\'s member list', () => {
  const v = memberView();
  assert.deepEqual(aiRoster(v), v.aiRoster);
  assert.equal(nationRoster(v), v.roster);
  // A play server from before aiRoster sent the object as roster (and a spectator view has no member list).
  const old = { roster: { aiCount: 2, bountyEach: '1', homeTick: 30, revealed: true, fallen: [] } };
  assert.equal(aiRoster(old).aiCount, 2);
  assert.deepEqual(nationRoster(old), []);
  assert.deepEqual(aiRoster({ roster: v.roster }), { aiCount: 0, bountyEach: '0', homeTick: null, revealed: false, fallen: [] }, 'never the member list');
  assert.equal(aiRoster(undefined).aiCount, 0);
});

test('the talk drawer shows the AI members, their bounty and the fallen homes to a member', () => {
  const v = memberView();
  Object.assign(S, { view: v, myCiv: 0, memberId: 0, session: null, watch: null });
  const markup = String(drawerTalk());
  assert.match(markup, /運営のAIメンバーが <b>4人<\/b>/);
  assert.match(markup, /懸賞金 <b>5\.00 USDC<\/b>/);
  assert.match(markup, /ティック45に決まる/);
  assert.match(markup, /<b>Ilse<\/b>/, 'the fallen AI member is named');
  assert.doesNotMatch(markup, /まだ落ちた住まいはありません/);
  assert.doesNotMatch(markup, /<b>人<\/b>|undefined/);
});

test('the help text counts the AI members from the game view', () => {
  assert.match(othersText(memberView()), /うち4人は運営のAIメンバーです/);
  assert.doesNotMatch(othersText({ ...memberView(), aiRoster: { aiCount: 0, fallen: [] } }), /運営のAIメンバー/);
});
