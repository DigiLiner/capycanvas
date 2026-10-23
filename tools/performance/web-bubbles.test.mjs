import assert from 'node:assert/strict';
import {test} from 'node:test';
import {parseSurfaceLatency,summarizeIntervals} from './web-bubbles.mjs';
test('SurfaceFlinger timestamps exclude empty and pending slots and preserve real presents',()=>{
  assert.deepEqual(parseSurfaceLatency('11111111\n0\t0\t0\n1 9223372036854775807 3\n1 2303751150401536 3\n1 2303751150401536 3\n'),{refreshNs:11111111,actualNs:[2303751150401536]});
  assert.throws(()=>parseSurfaceLatency('not a refresh period'));
  assert.throws(()=>parseSurfaceLatency('11111111\n1 2'));
});
test('Motion intervals retain long gaps',()=>{
  assert.equal(summarizeIntervals([0,10,20,120]).raf_gap_p99_ms,100);
  assert.equal(summarizeIntervals([]).raf_proxy_hz,null);
});
