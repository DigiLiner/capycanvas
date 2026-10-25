import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {scopesFixture} from './scopes-fixture.mjs';
import {createDrawing} from './new-drawing-fixture.mjs';

export function installNativeWritebackTrace() {
  const trace=globalThis.nativeWritebackTrace={limits:[],layouts:[],dispatches:[]};
  const layouts=new WeakMap(),groups=new WeakMap(),pipelines=new WeakMap(),passes=new WeakMap();
  const wrap=(prototype,name,record)=>{const original=prototype[name];prototype[name]=function(...args){const value=original.apply(this,args);return record(value,args)??value;};};
  wrap(GPUAdapter.prototype,'requestDevice',(value)=>value.then(device=>{
    trace.limits.push({maxStorageBuffersPerShaderStage:device.limits.maxStorageBuffersPerShaderStage,maxStorageTexturesPerShaderStage:device.limits.maxStorageTexturesPerShaderStage});return device;
  }));
  wrap(GPUDevice.prototype,'createBindGroupLayout',(value,[descriptor])=>{
    if(descriptor.label!=='native tile encoder inputs')return;
    const entries=Array.from(descriptor.entries),record={storageBuffers:entries.filter(e=>e.buffer?.type?.startsWith('storage')||e.buffer?.type==='read-only-storage').length,sampledTextures:entries.filter(e=>e.texture).length,storageFormats:entries.filter(e=>e.storageTexture).map(e=>e.storageTexture.format)};
    layouts.set(value,record);trace.layouts.push(record);
  });
  wrap(GPUDevice.prototype,'createPipelineLayout',(value,[descriptor])=>{groups.set(value,Array.from(descriptor.bindGroupLayouts).map(layout=>layouts.get(layout)));});
  for(const name of ['createComputePipeline','createComputePipelineAsync'])wrap(GPUDevice.prototype,name,(value,[descriptor])=>{
    const record=groups.get(descriptor.layout)?.find(Boolean);
    if(!record)return;
    if(value instanceof Promise)return value.then(pipeline=>{pipelines.set(pipeline,record);return pipeline;});
    pipelines.set(value,record);
  });
  const setPipeline=GPUComputePassEncoder.prototype.setPipeline;
  GPUComputePassEncoder.prototype.setPipeline=function(pipeline){passes.set(this,pipelines.get(pipeline));return setPipeline.call(this,pipeline);};
  const dispatch=GPUComputePassEncoder.prototype.dispatchWorkgroups;
  GPUComputePassEncoder.prototype.dispatchWorkgroups=function(...args){const record=passes.get(this);if(record)trace.dispatches.push(record);return dispatch.apply(this,args);};
}

export async function checkNativeWriteback({call,evaluate,settle}) {
  const fixture=await scopesFixture({call,evaluate,settle});
  const {send,invoke,poll,idle,checkpoint,documentPoint,reopen,dispose}=fixture;
  const directory=process.env.LAYER_TEST_ARTIFACTS??'artifacts/native-writeback/web';
  await mkdir(directory,{recursive:true});
  const cases=[];
  try {
    for(const mode of [1,2]) {
      await invoke('new_document');await evaluate(createDrawing({width:768,height:512}));await idle();await invoke('fit_canvas');
      const layer=await evaluate('Number(layerApp.state().layer_tools.editing_layer.id)');
      await send({type:'effect',action:{op:'set',layer,key:'color_mode',value:{kind:'choice',value:mode}}});
      await poll(`layerApp.state().layer_properties.controls.find(control=>control.key==='color_mode')?.value.value===${mode}`);
      await invoke('pen');await send({type:'select_brush',id:1});await send({type:'set_brush_size',value:160});await send({type:'set_color',rgba:[0,0,0,1]});await idle();
      const before=await checkpoint(),revision=await evaluate('String(layerApp.state().layers.find(l=>l.editing).paint_revision)'),dispatchStart=await evaluate('nativeWritebackTrace.dispatches.length');
      for(let i=0;i<=12;i++) {
        const at=await documentPoint([96+i*48,256]);
        await call('Input.dispatchMouseEvent',{type:i===0?'mousePressed':'mouseMoved',...at,button:'left',buttons:1,clickCount:1,pointerType:'pen',force:1});await settle();
      }
      await call('Input.dispatchMouseEvent',{type:'mouseReleased',...await documentPoint([672,256]),button:'left',buttons:0,clickCount:1,pointerType:'pen',force:0});
      await poll(`String(layerApp.state().layers.find(l=>l.editing).paint_revision)!==${JSON.stringify(revision)}`);await idle();
      const painted=await checkpoint();assert.notDeepEqual(painted.rasters,before.rasters,`mode ${mode}: committed native tiles`);
      const dispatches=await evaluate(`nativeWritebackTrace.dispatches.slice(${dispatchStart})`);
      assert.ok(dispatches.some(dispatch=>dispatch.storageFormats.length===1&&dispatch.storageFormats[0]==='rgba32float'&&dispatch.storageBuffers>=3),`mode ${mode}: native publication dispatched: ${JSON.stringify(dispatches)}`);
      const samples=[];
      for(const x of [128,384,640]) {
        await invoke('hand');const at=await documentPoint([x,256]);
        await call('Input.dispatchMouseEvent',{type:'mouseMoved',...at,buttons:0,pointerType:'mouse'});await settle();await invoke('eyedropper');
        await poll('layerApp.state().color_picker.preview!==null');
        const rgba=await evaluate('Array.from(layerApp.state().color_picker.preview.rgba)');
        samples.push({x,rgba});
        assert.ok(rgba.slice(0,3).every(v=>Math.abs(v)<.005)&&Math.abs(rgba[3]-1)<.005,`mode ${mode}, x ${x}: black opaque stroke ${rgba}`);
      }
      await invoke('undo');await idle();assert.deepEqual(await checkpoint(),before,`mode ${mode}: exact Undo`);
      await invoke('redo');await idle();assert.deepEqual(await checkpoint(),painted,`mode ${mode}: exact Redo`);
      await reopen();assert.deepEqual(await checkpoint(),painted,`mode ${mode}: saved native tiles reopen exactly`);
      cases.push({mode,dispatches,samples});
    }
    const trace=await evaluate('nativeWritebackTrace');
    await writeFile(`${directory}/native-writeback.json`,JSON.stringify({...trace,cases},null,2));
    assert.ok(trace.limits.length,'actual browser device limits recorded');
    assert.ok(trace.layouts.length,'native writeback layouts recorded');
    assert.ok(trace.layouts.every(layout=>layout.storageBuffers<=4),'native encoder recipes fit the portable four-buffer profile');
    console.log('PASS native grayscale/two-tone writeback, pixels, exact Undo/Redo and reopen',trace.limits,`${trace.dispatches.length} native dispatches`);
  } catch(error) {await writeFile(`${directory}/native-writeback-failure.json`,JSON.stringify(await evaluate('nativeWritebackTrace'),null,2));throw error;
  } finally {await dispose();}
}
