import test from 'node:test';
import assert from 'node:assert/strict';
import { chronicleText } from './copy.mjs';

test('old system world names are updated only in presentation, preserving the saved event', () => {
  const event = {type:'CITIZEN_JOINED',text:'Maraがアスターの市民になった。'};
  assert.equal(chronicleText(event), 'Maraが文明の市民になった。');
  assert.equal(event.text, 'Maraがアスターの市民になった。');
  assert.equal(chronicleText({type:'AMBITION_REACHED',text:'アスターは開拓期の共同目標を達成した。文明の発展はこのまま続く。'}), '私たちの文明は開拓期の共同目標を達成した。文明の発展はこのまま続く。');
});

test('player names and unrelated messages are never globally renamed', () => {
  assert.equal(chronicleText({type:'CITIZEN_JOINED',text:'アスターがアスターの市民になった。'}), 'アスターが文明の市民になった。');
  const event = {type:'BUILD_STARTED',text:'Asterが農場に投資した。'};
  assert.equal(chronicleText(event), event.text);
  assert.equal(chronicleText({type:'CITIZEN_JOINED',text:'Maraが文明の市民になった。'}), 'Maraが文明の市民になった。');
});
