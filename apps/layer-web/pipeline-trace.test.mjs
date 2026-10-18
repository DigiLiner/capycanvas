import assert from 'node:assert/strict';
import {prepareWorkers} from '../../tools/cdp.mjs';

export async function tracePipelineCalls(cdp) {
  const calls=[],jobs=[],workers=new Set(),detached=new Set();
  let holdTransforms=false;
  const held=new Set(),liveWorkers=new Set();
  const debug=message=>{if(process.env.LAYER_TEST_VERBOSE)console.log('Pipeline trace:',message)};
  const source=`(()=>{
    if(globalThis.__pipelineTraceInstalled)return;
    globalThis.__pipelineTraceInstalled=true;
    const realm=typeof document==='undefined'?'worker':'page';
    const identities=new WeakMap();let nextIdentity=0;
    const identity=value=>{if(!identities.has(value))identities.set(value,++nextIdentity);return identities.get(value)};
    globalThis.__pipelineTraceFingerprint=(device,descriptor)=>identity(device)+':'+JSON.stringify(descriptor,(_key,value)=>{
      if(value&&typeof value==='object'&&!Array.isArray(value)&&![Object.prototype,null].includes(Object.getPrototypeOf(value)))return {gpuObject:identity(value)};
      return value;
    });
    globalThis.__capyPipelineTrace(JSON.stringify({realm,installed:true}));
    if(realm==='worker'){
      self.addEventListener('message',event=>{
        const request=event.data?.request;if(!request)return;
        let task;try{task=JSON.parse(request.metadata)[2]}catch{}
        globalThis.__pipelineTask=typeof task==='string'?task:task&&Object.keys(task)[0];
        globalThis.__capyPipelineTrace(JSON.stringify({realm,operation:request.operation,task:globalThis.__pipelineTask}));
      });
      for(const method of ['instantiate','instantiateStreaming']){
        const original=WebAssembly[method];
        WebAssembly[method]=function(...args){
          if(globalThis.__pipelineTask==='TransformPixels')globalThis.__capyPipelineTrace(JSON.stringify({realm,task:globalThis.__pipelineTask,initialization:method,module:args[0] instanceof WebAssembly.Module}));
          return original.apply(this,args);
        };
      }
      if(navigator.gpu){
        const requestAdapter=navigator.gpu.requestAdapter;
        navigator.gpu.requestAdapter=async function(...args){
          const adapter=await requestAdapter.apply(this,args);if(!adapter)return adapter;
          const requestDevice=adapter.requestDevice;
          adapter.requestDevice=async function(...values){
            const device=await requestDevice.apply(this,values);
            for(const method of ['createComputePipeline','createRenderPipeline','createComputePipelineAsync','createRenderPipelineAsync']){
              const original=device[method];
              device[method]=function(descriptor){
                globalThis.__capyPipelineTrace(JSON.stringify({realm,task:globalThis.__pipelineTask,method,label:descriptor.label,recipe:__pipelineTraceFingerprint(this,descriptor)}));
                const result=original.call(this,descriptor);
                if(method.endsWith('Async')&&globalThis.__pipelineTask==='TransformPixels'&&globalThis.__holdTransform)return result.then(pipeline=>new Promise(resolve=>{
                  (globalThis.__transformReleases??=[]).push(()=>resolve(pipeline));
                  globalThis.__capyPipelineTrace(JSON.stringify({realm,held:true,label:descriptor.label}));
                }));
                return result;
              };
            }
            return device;
          };
          return adapter;
        };
      }
      return;
    }
    if(typeof GPUDevice==='undefined')return;
    for(const method of ['createComputePipeline','createRenderPipeline','createComputePipelineAsync','createRenderPipelineAsync']){
      const original=GPUDevice.prototype[method];
      GPUDevice.prototype[method]=function(descriptor){
        globalThis.__capyPipelineTrace(JSON.stringify({realm,method,label:descriptor.label,recipe:__pipelineTraceFingerprint(this,descriptor)}));
        return original.call(this,descriptor);
      };
    }
  })()`;
  const preparation=prepareWorkers(cdp,async session=>{
    await cdp.call('Runtime.addBinding',{name:'__capyPipelineTrace'},session);
    const result=await cdp.call('Runtime.evaluate',{expression:source+';globalThis.__holdTransform='+holdTransforms},session);
    if(result.exceptionDetails)throw Error(JSON.stringify(result.exceptionDetails));
    debug('injected '+session);
  });
  const receive=cdp.receive;
  cdp.receive=message=>{
    receive(message);
    if(message.method==='Runtime.bindingCalled'&&message.params.name==='__capyPipelineTrace'){
      const record=JSON.parse(message.params.payload);
      if(record.installed&&record.realm==='worker')workers.add(message.sessionId);
      if(record.method)calls.push({...record,session:message.sessionId});
      if(record.operation||record.initialization)jobs.push({...record,session:message.sessionId});
      if(record.held)held.add(message.sessionId);
    }
    if(message.method==='Target.detachedFromTarget'){detached.add(message.params.sessionId);debug('detached '+message.params.sessionId)}
    if(message.method==='Target.attachedToTarget'&&message.params.targetInfo.type==='worker'){
      const session=message.params.sessionId;
      liveWorkers.add(session);
      debug('attached '+session+' '+message.params.targetInfo.url);
    }
  };
  await cdp.call('Runtime.addBinding',{name:'__capyPipelineTrace'});
  const {identifier}=await cdp.call('Page.addScriptToEvaluateOnNewDocument',{source});
  await preparation.start();
  debug('auto-attach ready');
  return {
    calls,jobs,
    async holdTransforms(){
      holdTransforms=true;held.clear();
      for(const session of liveWorkers)if(!detached.has(session))await cdp.call('Runtime.evaluate',{expression:'globalThis.__holdTransform=true'},session);
    },
    async waitHeld(){
      const deadline=Date.now()+120000;
      while(!held.size&&Date.now()<deadline)await new Promise(resolve=>setTimeout(resolve,25));
      assert.ok(held.size,'Tool switch reached held genuine worker transform compilation');
    },
    async releaseTransforms(){
      holdTransforms=false;
      for(const session of liveWorkers)if(!detached.has(session))await cdp.call('Runtime.evaluate',{expression:'globalThis.__holdTransform=false;for(const release of globalThis.__transformReleases??[])release();globalThis.__transformReleases=[]'},session);
      held.clear();
    },
    async check(stage,{worker=false}={}){
      await preparation.check();
      if(worker){
        assert.ok(jobs.some(record=>record.task==='TransformPixels'),`${stage}: native paint TransformPixels worker job was observed`);
        assert.ok(calls.some(record=>record.realm==='worker'),`${stage}: worker GPU pipelines were observed`);
      }
      assert.ok(workers.size>0,`${stage}: worker boundary instrumentation was installed`);
      assert.ok(calls.some(record=>record.realm==='page'),`${stage}: page GPU pipelines were observed`);
      assert.deepEqual(calls.filter(record=>!record.method.endsWith('Async')),[],`${stage}: no immediate GPU pipeline creation in page or workers`);
    },
    async close(){
      if(holdTransforms)await this.releaseTransforms();
      try { await preparation.close(); }
      finally {
        try { await cdp.call('Page.removeScriptToEvaluateOnNewDocument',{identifier}); }
        finally { cdp.receive=receive; }
      }
    },
  };
}
