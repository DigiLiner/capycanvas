import assert from 'node:assert/strict';
import test from 'node:test';
import {createDocumentRecovery} from './document-recovery.js';

test('dismissing a failed drawing leaves its blocked checkpoint available without retrying or discarding',async t=>{
  const previous={document:globalThis.document,window:globalThis.window,navigator:Object.getOwnPropertyDescriptor(globalThis,'navigator')};
  globalThis.document={addEventListener(){},hidden:false};globalThis.window={addEventListener(){}};
  Object.defineProperty(globalThis,'navigator',{configurable:true,value:{locks:{request:(_name,_options,callback)=>typeof _options==='function'?_options({}):callback({})}}});
  t.after(()=>{globalThis.document=previous.document;globalThis.window=previous.window;if(previous.navigator)Object.defineProperty(globalThis,'navigator',previous.navigator);else delete globalThis.navigator;});
  t.mock.method(globalThis,'setInterval',()=>0);
  const events=[],operations=[],stored={generation:1,drawings:[{id:1,key:'blocked'}],active:1,blocked:[1],restoring:[],clean_exit:false};
  const app={document_tabs:()=>({tabs:[],selected:0}),session_stamp_for:()=>({}),session_checkpoint_interval:()=>2000,
    session_manifest_update:(raw,event)=>{events.push(event);const value=raw?JSON.parse(raw):{generation:0,drawings:[],blocked:[],restoring:[]};return {...value,...(event.type==='reconcile'?{drawings:event.drawings,active:event.active}:{}),generation:value.generation+1};}};
  const recovery=createDocumentRecovery({app,failed:async()=>null,restore:()=>assert.fail('dismissal cannot restore'),message:()=>{},call:async request=>{
    operations.push(request.operation);
    if(request.operation==='restart-store-list')return ['abandoned'];
    if(request.operation==='restart-store-manifest')return stored;
    if(request.operation==='restart-store-publish-manifest'||request.operation==='restart-retain')return;
    assert.fail(`Unexpected recovery operation ${request.operation}`);
  }});
  await recovery.start();
  assert.equal(events.some(event=>['retry_restore','remove','begin_restore'].includes(event.type)),false);
  assert.equal(operations.includes('restart-store-read'),false);
  assert.deepEqual(stored.blocked,[1]);
});
