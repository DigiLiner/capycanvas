import assert from 'node:assert/strict';
import test from 'node:test';
import {createWorkflowStack} from './workflow-stack.js';

function page(width,height){const classes=new Set();return {hidden:false,inert:false,width,height,classList:{add(...items){items.forEach(value=>classes.add(value));},remove(...items){items.forEach(value=>classes.delete(value));},toggle(value,on){if(on)classes.add(value);else classes.delete(value);},contains:value=>classes.has(value)}};}
function fixture(t,reduced=false){
  const previous={requestAnimationFrame:globalThis.requestAnimationFrame,cancelAnimationFrame:globalThis.cancelAnimationFrame,matchMedia:globalThis.matchMedia,getComputedStyle:globalThis.getComputedStyle};
  const frames=new Map(),timers=new Map();let sequence=0;
  Object.assign(globalThis,{requestAnimationFrame:callback=>{const id=++sequence;frames.set(id,callback);return id;},cancelAnimationFrame:id=>frames.delete(id),matchMedia:()=>({matches:reduced}),getComputedStyle:()=>({transitionDuration:'0.2s'})});
  t.after(()=>Object.assign(globalThis,previous));
  t.mock.method(globalThis,'setTimeout',callback=>{const id=++sequence;timers.set(id,callback);return id;});t.mock.method(globalThis,'clearTimeout',id=>timers.delete(id));
  const root=page(0,0);Object.assign(root,{dataset:{},style:{setProperty(name,value){this[name]=value;}},children:[page(100,40),page(180,60),page(120,80)],getBoundingClientRect(){const shown=this.children.find(child=>!child.hidden);return {width:shown?.width??0,height:shown?.height??0};}});
  return {root,flush(){for(const [id,callback]of [...frames]){frames.delete(id);callback();}},finish(){for(const [id,callback]of [...timers]){timers.delete(id);callback();}},pending:()=>timers.size};
}

test('interrupted page transitions retain one input owner and retire outgoing pages',t=>{
  const f=fixture(t),stack=createWorkflowStack(f.root,'slide'),[first,second,third]=f.root.children;
  stack.show(first);stack.show(second);assert.equal(first.hidden,false);assert.equal(first.inert,true);assert.equal(second.inert,false);f.flush();assert.equal(f.pending(),1);
  stack.show(third);f.flush();f.finish();
  assert.equal(third.hidden,false);assert.equal(third.inert,false);assert.ok([first,second].every(page=>page.hidden&&page.inert));assert.equal(f.root.classList.contains('transitioning'),false);assert.equal(f.root.style.width,'');assert.equal(f.root.style.height,'');
});

test('reduced motion switches pages immediately without scheduling presentation work',t=>{
  const f=fixture(t,true),stack=createWorkflowStack(f.root),[first,second]=f.root.children;stack.show(first);stack.show(second);
  assert.equal(first.hidden,true);assert.equal(second.hidden,false);assert.equal(f.pending(),0);assert.equal(f.root.classList.contains('transitioning'),false);
});
