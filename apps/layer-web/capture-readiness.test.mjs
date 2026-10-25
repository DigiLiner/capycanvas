import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {prepareWorkers} from '../../tools/cdp.mjs';
import {scopesFixture} from './scopes-fixture.mjs';
import {histogramJourney} from './histogram-journey.mjs';

async function captureTrace(cdp) {
  const records=[],sessions=new Set();
  const source=`(()=>{
      if(globalThis.__captureTraceInstalled)return;globalThis.__captureTraceInstalled=true;
      const modules=new WeakMap(),pipelines=new WeakMap(),passes=new WeakMap();
      const originals=[...['createShaderModule','createComputePipeline','createComputePipelineAsync','createRenderPipeline','createRenderPipelineAsync'].map(key=>[GPUDevice.prototype,key]),...[GPUComputePassEncoder,GPURenderPassEncoder].flatMap(pass=>['setPipeline','dispatchWorkgroups','dispatchWorkgroupsIndirect','draw','drawIndexed','drawIndirect','drawIndexedIndirect'].filter(key=>pass.prototype[key]).map(key=>[pass.prototype,key]))].map(([object,key])=>[object,key,object[key]]);
      globalThis.__captureTraceRestore=()=>{for(const [object,key,value] of originals)object[key]=value;delete globalThis.__captureTraceInstalled};
      const emit=record=>__captureTrace(JSON.stringify({realm:typeof document==='undefined'?'worker':'page',...record}));
      const createModule=GPUDevice.prototype.createShaderModule;
      GPUDevice.prototype.createShaderModule=function(d){const module=createModule.call(this,d);const body=d.code.slice(d.code.indexOf('fn effect_value('));modules.set(module,{label:d.label,chain:(body.match(/fx_begin\\(\\d+u\\)/g)||[]).length});return module};
      for(const method of ['createComputePipeline','createComputePipelineAsync','createRenderPipeline','createRenderPipelineAsync']){
        const original=GPUDevice.prototype[method];GPUDevice.prototype[method]=function(d){const recipe={...modules.get((d.compute??d.fragment??d.vertex).module),pipeline:d.label,entry:(d.compute??d.fragment??d.vertex).entryPoint,method};emit({event:'compile',...recipe});const value=original.call(this,d);if(method.endsWith('Async'))return value.then(p=>{pipelines.set(p,recipe);emit({event:'ready',...recipe});return p});pipelines.set(value,recipe);return value};
      }
      for(const pass of [GPUComputePassEncoder,GPURenderPassEncoder]){
      const set=pass.prototype.setPipeline;
      pass.prototype.setPipeline=function(p){passes.set(this,pipelines.get(p));return set.call(this,p)};
      for(const method of ['dispatchWorkgroups','dispatchWorkgroupsIndirect','draw','drawIndexed','drawIndirect','drawIndexedIndirect']){const original=pass.prototype[method];if(!original)continue;pass.prototype[method]=function(...args){const recipe=passes.get(this);if(recipe&&(recipe.chain||recipe.entry==='prep_main'))emit({event:'dispatch',...recipe});return original.apply(this,args)}}}
    })()`;
  const install=async session=>{sessions.add(session);
    await cdp.call('Runtime.addBinding',{name:'__captureTrace'},session);
    const result=await cdp.call('Runtime.evaluate',{expression:source},session);
    if(result.exceptionDetails)throw Error(JSON.stringify(result.exceptionDetails));
  };
  const preparation=prepareWorkers(cdp,install);
  const receive=cdp.receive;
  cdp.receive=message=>{receive(message);if(message.method==='Runtime.bindingCalled'&&message.params.name==='__captureTrace')records.push({...JSON.parse(message.params.payload),session:message.sessionId})};
  await install();await preparation.start();
  return {records,check:()=>preparation.check(),async close(){try{await preparation.close();await Promise.allSettled([...sessions].map(session=>cdp.call('Runtime.evaluate',{expression:'globalThis.__captureTraceRestore?.()'},session)))}finally{cdp.receive=receive}}};
}

export async function checkCaptureReadiness({cdp,call,evaluate,settle}) {
  const trace=await captureTrace(cdp),fixture=await scopesFixture({call,evaluate,settle});
  const histogram=histogramJourney({evaluate,settle});
  const {send,poll,json}=fixture;
  const directory=process.env.LAYER_TEST_ARTIFACTS??'artifacts/ui/capture-readiness-web';
  const results=[];
  const owner=()=>evaluate('Number(layerApp.state().layer_tools.editing_layer.id)');
  const insert=async effect=>{await send({type:'effect',action:{op:'insert',effect}});return owner()};
  const hide=async()=>{await histogram.hide();await send({type:'customize',action:{type:'set_panel_visible',panel:'properties',visible:false}})};
  const cold=async()=>{await hide();await evaluate('new Promise(resolve=>setTimeout(resolve,6000))');await settle()};
  const endpoint=(data,bin,label)=>{
    assert.equal(data.pixels,256*256,`${label}: complete opaque source`);assert.equal(data.transparent,0);
    for(const channel of data.channels.slice(0,3)){assert.equal(channel.bins[bin],data.pixels,`${label}: RGB endpoint ${bin}`);assert.equal(channel.bins.reduce((a,b)=>a+b,0),data.pixels)}
  };
  try {
    await evaluate(`(async()=>{const c=new OffscreenCanvas(256,256),x=c.getContext('2d'),image=x.createImageData(256,256);for(let y=0;y<256;y++)for(let i=0;i<256;i++){const v=((i>>4)^(y>>4))&1?255:0;image.data.set([v,v,v,255],(y*256+i)*4)}x.putImageData(image,0,0);scopePhoto=new File([await c.convertToBlob({type:'image/png'})],'capture-checker.png',{type:'image/png'});window.showOpenFilePicker=async()=>[{name:scopePhoto.name,getFile:async()=>scopePhoto}]})()`);
    const epoch=await evaluate('String(layerApp.state().document_file.epoch)');
    await fixture.invoke('open_document');await poll(`String(layerApp.state().document_file.epoch)!==${JSON.stringify(epoch)}`);await fixture.idle();
    await hide();await send({type:'set_color',rgba:[0,0,0,1]});const fill=await insert('solid_color');
    await insert('invert');await insert('invert');const hidden=await insert('invert');
    await send({type:'layer',action:{op:'visibility',id:hidden,value:false}});
    await insert('invert');await insert('invert');
    assert.equal(await evaluate("layerApp.state().layers.filter(l=>l.label==='Invert').length"),5,'Five authored inversions retain the hidden separator');
    assert.equal(await evaluate(`layerApp.state().layers.find(l=>Number(l.id)===${hidden}).visible`),false);
    await cold();let start=trace.records.length;
    const separated=await histogram.exact();endpoint(separated,0,'Hidden separator');
    await trace.check();let events=trace.records.slice(start);
    assert.ok(events.some(e=>e.event==='dispatch'&&e.chain===2),'Cold visible capture executes actual two-effect segments');
    assert.ok(events.some(e=>e.event==='ready'&&e.chain===2),'Cold visible capture awaits two-effect pipeline preparation');
    results.push({case:'hidden-separator',histogram:separated,events});
    console.log('Cold capture: hidden separator exact black and actual async two-effect dispatch passed');
    await hide();
    await send({type:'layer',action:{op:'visibility',id:fill,value:false}});
    const bakeHistogram=await histogram.exact();
    assert.ok(bakeHistogram.channels.slice(0,3).every(c=>c.bins.filter(n=>n>0).length>1),'Bake fixture retains actual nonconstant source pixels');
    await hide();
    const beforeBake=await fixture.checkpoint(),beforeLayers=await json('layerApp.state().layers');
    await cold();start=trace.records.length;
    const merge=await evaluate("layerApp.state().commands.find(c=>c.label==='Merge Visible')");
    assert.ok(merge?.enabled,'Public Merge Visible is enabled for the valid effect stack');
    await fixture.invoke(merge.id);
    await poll(`!layerApp.documents.busy()&&!layerApp.state().document_file.busy&&layerApp.state().layers.length<${beforeLayers.length}`);
    await fixture.idle();
    const baked=await fixture.checkpoint();assert.notDeepEqual(baked.authored,beforeBake.authored,'Bake publishes a new authored paint result');
    assert.notDeepEqual(baked.rasters,beforeBake.rasters,'Bake publishes actual raster storage');
    assert.deepEqual(await histogram.exact(),bakeHistogram,'Cold Web Bake preserves every exact source histogram bin');
    await trace.check();events=trace.records.slice(start);
    assert.ok(events.some(e=>e.event==='ready'&&e.chain===4),'Cold Bake asynchronously prepares its visible-member four-effect recipe');
    assert.ok(events.some(e=>e.event==='dispatch'&&e.chain===4),'Cold Bake executes its visible-member four-effect recipe on its GPU owner');
    results.push({case:'cold-bake',histogram:await json('layerApp.state().histogram.data'),events});
    await hide();await fixture.invoke('undo');await fixture.idle();
    assert.deepEqual(await fixture.checkpoint(),beforeBake,'One Undo restores exact source, authored data and raster identities after Bake');
    console.log('Cold Bake: exact checkerboard histogram, raster publication, recipe dispatch and Undo passed');
    await fixture.invoke('undo');
    for(let i=0;i<6;i++)await send({type:'invoke',command:'undo'});
    assert.equal(await evaluate("layerApp.state().layers.filter(l=>l.label==='Invert').length"),0,'Prefix fixture starts from the retained black fill');
    await insert('invert');await insert('invert');const target=await insert('curves');
    for(const key of ['red','green','blue'])await send({type:'effect',action:{op:'set',layer:target,key,value:{kind:'curve',value:[[0,1],[1,0]]}}});
    await send({type:'effect',action:{op:'select_page',layer:target,page:'rgb'}});
    await cold();start=trace.records.length;
    await send({type:'customize',action:{type:'set_panel_visible',panel:'properties',visible:true}});
    await evaluate(`(()=>{const g=layerApp.app.layout(innerWidth,innerHeight).groups.find(g=>g.panels.includes('properties'));if(g&&g.active!=='properties')layerApp.dispatch({type:'select_panel_tab',group:g.id,panel:'properties'})})()`);await settle();
    await poll(`layerApp.state().tonal_histogram.status==='Exact'&&Number(layerApp.state().tonal_histogram.captured_source?.EffectChannels)===${target}&&layerApp.state().tonal_histogram.data!=null`);
    const prefix=await json('layerApp.state().tonal_histogram.data');endpoint(prefix,255,'Strict input prefix');
    await trace.check();events=trace.records.slice(start);
    assert.ok(events.some(e=>e.event==='dispatch'&&e.chain===2),'EffectChannels capture executes actual two-effect input prefix');
    assert.ok(events.some(e=>e.event==='ready'&&e.chain===2),'EffectChannels capture awaits prefix pipeline preparation');
    results.push({case:'effect-channels-prefix',histogram:prefix,events});
    console.log('Cold capture: EffectChannels publishes exact white after actual async strict-prefix dispatch');
    await hide();const blur=await insert('gaussian_blur');
    await send({type:'effect',action:{op:'set',layer:blur,key:'sigma',value:{kind:'number',value:3}}});
    await cold();start=trace.records.length;
    const lookup=await histogram.exact();endpoint(lookup,255,'Prepared Gaussian lookup');
    await trace.check();events=trace.records.slice(start);
    assert.ok(events.some(e=>e.event==='ready'&&e.entry==='prep_main'),'Cold capture awaits actual lookup preparation pipeline');
    assert.ok(events.some(e=>e.event==='dispatch'&&e.entry==='prep_main'),'Cold capture dispatches actual lookup preparation');
    results.push({case:'lookup-preparation',histogram:lookup,events});
    assert.deepEqual(trace.records.filter(e=>e.event==='compile'&&!e.method.endsWith('Async')),[],'Cold capture and Bake never create an immediate GPU pipeline');
    console.log('PASS: cold hidden-separator, Bake, EffectChannels prefix and Gaussian lookup captures publish exact pixels after asynchronous recipe preparation');
  } finally {
    await mkdir(directory,{recursive:true});await writeFile(`${directory}/capture-readiness.json`,JSON.stringify({results,trace:trace.records},null,2));
    await fixture.dispose();await trace.close();
  }
}
