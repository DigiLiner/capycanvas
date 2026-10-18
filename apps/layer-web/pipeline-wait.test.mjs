import test from 'node:test';
import assert from 'node:assert/strict';
import {runInNewContext} from 'node:vm';
import {installPipelineWait} from './pipeline-wait.mjs';

for (const method of ['createComputePipelineAsync', 'createRenderPipelineAsync']) {
  test(`${method} releases pipelines that finish before and after release`, async () => {
    const native = [Promise.withResolvers(), Promise.withResolvers()];
    let next = 0;
    class GPUDevice {}
    const original = GPUDevice.prototype[method] = () => native[next++].promise;
    const realm = {GPUDevice, __pipelineTraceFingerprint: (_, descriptor) => descriptor.label};
    runInNewContext(`globalThis.window = globalThis; (${installPipelineWait.toString()})()`, realm);
    const device = new GPUDevice();
    const first = device[method]({label: 'first'});
    const second = device[method]({label: 'second'});
    const values = [{pipeline: 1}, {pipeline: 2}];
    native[0].resolve(values[0]);
    await native[0].promise;
    assert.equal(realm.pipelineWait.held.length, 1);
    assert.equal(realm.pipelineWait.calls.length, 2);
    realm.pipelineWait.restore();
    realm.pipelineWait.release();
    let laterResolved = false;
    second.then(() => { laterResolved = true; });
    native[1].resolve(values[1]);
    await native[1].promise;
    await Promise.resolve();
    assert.equal(laterResolved, true, 'release includes already-started native promises that settle later');
    assert.deepEqual(await Promise.all([first, second]), values);
    assert.equal(realm.pipelineWait.held.length, 0);
    assert.equal(GPUDevice.prototype[method], original);
  });
}
