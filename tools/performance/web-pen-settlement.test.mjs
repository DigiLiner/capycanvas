import test from 'node:test';
import assert from 'node:assert/strict';
import { photoDimensions, settleStroke } from './web-pen-settlement.mjs';

test('photo dimensions follow each tier image and reject mismatched expectations', () => {
  for (const [width, height] of [[4248, 2832], [6000, 4000], [9504, 6336]])
    assert.deepEqual(photoDimensions({ width, height }), { width, height });
  assert.throws(() => photoDimensions({ width: 9504, height: 6336 }, '4248', '2832'), /differs/);
  assert.throws(() => photoDimensions({ width: 9504, height: 6336 }, '9504'), /both/);
});

function clock() {
  let time = 0;
  return { now: () => time, delay: async ms => { time += ms; } };
}

test('waits beyond 500 ms for a committed stroke and its outstanding backing', async () => {
  const timer = clock();
  const result = await settleStroke(async () => ({ ready: true, revision: timer.now() >= 750 ? 2 : 1,
    session: { raster_backing_pending: timer.now() < 1000 } }), { before: 1, ...timer });
  assert.equal(result.elapsed_ms, 1000);
});

test('does not accept a revision while ordered input or canvas work remains', async () => {
  const timer = clock();
  const result = await settleStroke(async () => ({ ready: true, revision: 2,
    pending: timer.now() < 100, session: { deferred_edits: timer.now() < 200 ? 1 : 0 } }), { before: 1, ...timer });
  assert.equal(result.elapsed_ms, 200);
});

test('unchanged paint fails with bounded outstanding-work diagnostics', async () => {
  const timer = clock();
  await assert.rejects(settleStroke(async () => ({ ready: true, revision: 4, session: { input_pending: false } }),
    { before: 4, timeoutMs: 1000, ...timer }), /within 1000 ms.*"before":4.*"revision":4/);
  assert.equal(timer.now(), 1000);
});

test('GPU failure is reported immediately instead of waiting for the deadline', async () => {
  const timer = clock();
  await assert.rejects(settleStroke(async () => ({ failure: 'device lost' }), { before: 1, ...timer }), /device lost/);
  assert.equal(timer.now(), 0);
});
