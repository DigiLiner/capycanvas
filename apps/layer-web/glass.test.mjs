import assert from 'node:assert/strict';
import test from 'node:test';
import {createGlass} from './glass.js';

test('bubble motion retains stationary glass measurements and publishes changing strength',()=>{
  let measured=0,strength=1,enabled=true;
  const calls=[],style={visibility:'visible',borderTopLeftRadius:'8',borderTopRightRadius:'8',borderBottomLeftRadius:'8',borderBottomRightRadius:'8'};
  const node={closest:()=>false,getBoundingClientRect:()=>{measured++;return {x:4,y:8,width:80,height:40};}};
  globalThis.CSS={supports:()=>true};
  globalThis.getComputedStyle=()=>style;
  globalThis.document={querySelectorAll:()=>[]};
  const glass=createGlass({
    app:{set_glass:(...args)=>calls.push(args)},canvas:{getBoundingClientRect:()=>({x:0,y:0})},
    workspace:{querySelectorAll:()=>[node],addEventListener(){}},connections:()=>[],
    bubbles:()=>({boxes:[],arcs:[[50,50,53,28,0,1,strength]]}),enabled:()=>enabled,wake(){},
  });
  glass.flush();
  strength=.2;glass.queueBubbles();glass.flush();
  assert.equal(measured,1);
  assert.equal(calls[0][0],calls[1][0]);
  assert.equal(calls[1][2].arcs[0][6],.2);
  glass.queue();glass.flush();assert.equal(measured,2);
  enabled=false;glass.queue();glass.flush();
  assert.equal(calls.at(-1)[0].length,0);
  assert.equal(calls.at(-1)[2],null);
});
