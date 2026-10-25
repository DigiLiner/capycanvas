import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';

export function installHealPortabilityTrace() {
  const trace=globalThis.healPortabilityTrace={layouts:[],dispatches:[]};
  const bindLayouts=new WeakMap(),pipelineLayouts=new WeakMap(),pipelines=new WeakMap(),groups=new WeakMap(),passes=new WeakMap();
  let sequence=0;
  const wrap=(prototype,name,observe)=>{const original=prototype[name];prototype[name]=function(...args){const value=original.apply(this,args);return observe(value,args)??value;};};
  wrap(GPUDevice.prototype,'createBindGroupLayout',(layout,[descriptor])=>{bindLayouts.set(layout,{id:++sequence,entries:Array.from(descriptor.entries).length});});
  wrap(GPUDevice.prototype,'createBindGroup',(group,[descriptor])=>{groups.set(group,{layout:bindLayouts.get(descriptor.layout),entries:Array.from(descriptor.entries).length});});
  const createLayout=GPUDevice.prototype.createPipelineLayout;
  GPUDevice.prototype.createPipelineLayout=function(descriptor){
    const record=descriptor.label==='heal chosen source'?{groups:Array.from(descriptor.bindGroupLayouts).map(layout=>layout?bindLayouts.get(layout):null)}:null;
    if(record)trace.layouts.push(record);
    const layout=createLayout.call(this,descriptor);if(record)pipelineLayouts.set(layout,record);return layout;
  };
  for(const name of ['createComputePipeline','createComputePipelineAsync'])wrap(GPUDevice.prototype,name,(value,[descriptor])=>{
    const record=pipelineLayouts.get(descriptor.layout);if(!record)return;
    if(value instanceof Promise)return value.then(pipeline=>{pipelines.set(pipeline,record);return pipeline;});
    pipelines.set(value,record);
  });
  const state=pass=>{if(!passes.has(pass))passes.set(pass,{pipeline:null,bindings:{}});return passes.get(pass);};
  const setPipeline=GPUComputePassEncoder.prototype.setPipeline;
  GPUComputePassEncoder.prototype.setPipeline=function(pipeline){state(this).pipeline=pipelines.get(pipeline);return setPipeline.call(this,pipeline);};
  const setBindGroup=GPUComputePassEncoder.prototype.setBindGroup;
  GPUComputePassEncoder.prototype.setBindGroup=function(index,group,...args){state(this).bindings[index]=groups.get(group);return setBindGroup.call(this,index,group,...args);};
  for(const name of ['dispatchWorkgroups','dispatchWorkgroupsIndirect']) {
    const original=GPUComputePassEncoder.prototype[name];
    GPUComputePassEncoder.prototype[name]=function(...args){const current=state(this);if(current.pipeline)trace.dispatches.push({method:name,expected:current.pipeline.groups[1],bound:current.bindings[1]??null});return original.apply(this,args);};
  }
}

export async function checkHealPortability(evaluate,directory) {
  const trace=await evaluate('healPortabilityTrace');
  await writeFile(`${directory}/heal-portability.json`,JSON.stringify(trace,null,2));
  assert.ok(trace.layouts.length,'Spot Healing chosen-source pipeline layouts observed');
  assert.ok(trace.layouts.every(layout=>layout.groups[1]?.entries===0),'Spot Healing declares an explicit empty group 1 instead of a null layout slot');
  assert.ok(trace.dispatches.length,'real Spot Healing chosen-source dispatches observed');
  assert.ok(trace.dispatches.every(dispatch=>dispatch.bound?.entries===0&&dispatch.bound.layout?.id===dispatch.expected?.id),'every chosen-source dispatch binds its matching explicit empty group 1');
  console.log(`PASS Spot Healing portability: ${trace.layouts.length} explicit layouts, ${trace.dispatches.length} matching empty-group dispatches`);
}
