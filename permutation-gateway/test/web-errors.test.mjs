// The web client's Japanese error texts for chain, gateway and wallet codes
// (permutation-server/web/i18n.mjs errorText, api.mjs translateError), and
// the chain-mode viewer parameter (api.mjs withViewer).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { CHAIN_ERROR_JA, errorText } from '../../permutation-server/web/i18n.mjs';
import { translateError, withViewer } from '../../permutation-server/web/api.mjs';
import { CHAIN_ERRORS } from '../client/src/codec.mjs';

const JAPANESE = /[぀-ヿ一-鿿]/;

test('every code the spec lists has a Japanese text', () => {
  const codes = ['SessionInUse', 'KindHidden', 'SeasonFull', 'RegistrationClosed', 'WrongStatus', 'InvalidName', 'AlreadyInitialized', 'AlreadyClaimed',
    'NothingToClaim', 'InsufficientFunds', 'WalletRejected', 'BlockhashExpired', 'RateLimited', 'OperatorLowFunds', 'WrongTick', 'WrongPhase', 'TickFrozen',
    'CommitMismatch', 'NotOfficer', 'BadSignature', 'TooManySeals', 'BatchTooLarge'];
  for (const code of codes) {
    assert.match(CHAIN_ERROR_JA[code] ?? '', JAPANESE, code);
    assert.equal(errorText({ code, error: 'whatever' }), CHAIN_ERROR_JA[code], code);
  }
  // Every program error the web names is a real one.
  for (const code of ['AlreadyInitialized', 'AlreadyClaimed', 'NothingToClaim', 'SeasonFull', 'InvalidName', 'WrongStatus', 'WrongTick', 'TickFrozen', 'WrongPhase', 'CommitMismatch']) {
    assert.ok(CHAIN_ERRORS.includes(code), code);
  }
});

test('errorText: a program error named in a message, wallet and RPC wordings, and pass-through', () => {
  assert.equal(errorText('AlreadyClaimed: Error processing Instruction 0: custom program error: 0x14'), CHAIN_ERROR_JA.AlreadyClaimed);
  assert.equal(errorText({ code: null, error: 'x402 settlement failed: TickFrozen' }), CHAIN_ERROR_JA.TickFrozen);
  assert.equal(errorText({ code: 'HTTP500', error: 'Transfer: insufficient funds' }), CHAIN_ERROR_JA.InsufficientFunds);
  assert.equal(errorText('Blockhash not found'), CHAIN_ERROR_JA.BlockhashExpired);
  assert.equal(errorText('block height exceeded'), CHAIN_ERROR_JA.BlockhashExpired);
  assert.equal(errorText({ code: 4001, message: 'User rejected the request.' }), CHAIN_ERROR_JA.WalletRejected);
  assert.equal(errorText({ code: 'network', error: 'network' }), 'サーバーに届きませんでした');
  assert.equal(errorText('something new'), 'something new');
  assert.equal(errorText({ code: 'Mystery', error: 'raw text' }), 'raw text');
  assert.equal(errorText(null), '');
  // Lower-case words are not codes: "scheme/network" is not the network error.
  assert.equal(errorText('scheme/network'), 'scheme/network');
});

test('translateError keeps the play server texts and adds the chain codes', () => {
  assert.equal(translateError('orders cost 3, only 2 spendable'), '命令の枠が足りません（必要3・使える枠2）');
  assert.equal(translateError('TickFrozen'), 'このティックは締め切られました。次のティックで出し直してください');
  assert.equal(translateError('network'), 'サーバーに届きませんでした');
  assert.equal(translateError({ code: 'SessionInUse' }), CHAIN_ERROR_JA.SessionInUse);
  assert.equal(translateError('RegistrationClosed: registration is closed'), CHAIN_ERROR_JA.RegistrationClosed);
  assert.match(translateError('chain mode: sign in the browser'), JAPANESE);
  assert.equal(translateError('plain'), 'plain');
});

test('withViewer: chain members read their public view as ?member=M', () => {
  assert.equal(withViewer('/api/state', 4), '/api/state?member=4');
  assert.equal(withViewer('/api/preview/unit?id=3', 4), '/api/preview/unit?id=3&member=4');
  assert.equal(withViewer('/api/state?civ=2', 4), '/api/state?civ=2');
  assert.equal(withViewer('/api/state?member=1', 4), '/api/state?member=1');
  assert.equal(withViewer('/api/state', null), '/api/state');
  assert.equal(withViewer('/gw/season', 4), '/gw/season');
  assert.equal(withViewer('/api/state', 0), '/api/state?member=0');
});
