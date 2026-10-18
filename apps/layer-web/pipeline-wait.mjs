export function installPipelineWait() {
  const originals = {}, held = [];
  let released = false;
  window.pipelineWait = {
    calls: [], held,
    release() { released = true; for (const resolve of held.splice(0)) resolve(); },
    restore() { for (const [method, original] of Object.entries(originals)) GPUDevice.prototype[method] = original; },
  };
  for (const method of ['createComputePipelineAsync', 'createRenderPipelineAsync']) {
    const original = originals[method] = GPUDevice.prototype[method];
    GPUDevice.prototype[method] = function(descriptor) {
      pipelineWait.calls.push({method, label: descriptor.label, recipe: __pipelineTraceFingerprint(this, descriptor)});
      return original.call(this, descriptor).then(pipeline => released ? pipeline : new Promise(resolve => held.push(() => resolve(pipeline))));
    };
  }
}
