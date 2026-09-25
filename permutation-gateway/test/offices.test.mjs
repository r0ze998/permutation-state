import { test } from 'node:test';
import assert from 'node:assert/strict';
import { allowedOffices, officeOf, splitByOffice, submitOffice } from '../client/src/offices.mjs';
import { vectors } from './vectors.mjs';

test('allowedOffices equals role_allows_static for every order vector', () => {
  assert.equal(vectors.offices.length, vectors.orders.length);
  for (const v of vectors.offices) assert.deepEqual(allowedOffices(v.dto), v.offices, JSON.stringify(v.dto));
});

test('officeOf picks one of the allowed offices (unit orders by the unit)', () => {
  const view = { units: [{ id: 7, type: 'Settler' }, { id: 1, type: 'Archer' }] };
  for (const v of vectors.offices) {
    const office = officeOf(v.dto, view);
    if (v.dto.type === 'RevealRationale') assert.equal(office, null);
    else assert.ok(v.offices.includes(office), `${JSON.stringify(v.dto)} → ${office}`);
  }
  assert.equal(officeOf({ type: 'MoveUnit', unit: 7, path: [] }, view), 'Steward');
  assert.equal(officeOf({ type: 'MoveUnit', unit: 1, path: [] }, view), 'General');
  assert.equal(officeOf({ type: 'SetStanding', target: { kind: 'Unit', id: 7 }, rule: { kind: 'Clear' } }, view), 'Steward');
  assert.equal(officeOf({ type: 'SetStanding', target: { kind: 'City', id: 0 }, rule: { kind: 'Clear' } }, view), 'Steward');
  assert.equal(officeOf({ type: 'Nonsense' }, view), null);
});

test('consents go to another held office only if the program accepts them there', () => {
  const war = { type: 'ConsentWar', civ: 2 };
  const spend = { type: 'ConsentSpend', usdc: 5 };
  assert.equal(submitOffice(war, ['General', 'Steward'], null), 'General');
  assert.equal(submitOffice(war, ['Steward'], null), 'Steward');
  assert.equal(submitOffice(war, ['Science'], null), null, 'the science officer may not consent to war');
  assert.equal(submitOffice(spend, ['Science'], null), 'Science');
  assert.equal(submitOffice(spend, ['Diplomat'], null), null);
  // Unit orders never move to the other unit office.
  assert.equal(submitOffice({ type: 'MoveUnit', unit: 1, path: [] }, ['Steward'], { units: [{ id: 1, type: 'Archer' }] }), null);
});

test('splitByOffice: one list per held office, the rest not held', () => {
  const orders = [{ type: 'SetResearch', techs: [] }, { type: 'DeclareWar', civ: 1 }, { type: 'ConsentWar', civ: 1 }, { type: 'Raze', city: 3 }];
  const { byOffice, notHeld } = splitByOffice(orders, ['Science', 'Steward'], null);
  assert.deepEqual(byOffice.Science, [orders[0]]);
  assert.deepEqual(byOffice.Steward, [orders[2]]);
  assert.deepEqual(notHeld, [orders[1], orders[3]]);
});
