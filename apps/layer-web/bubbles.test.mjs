import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import test from 'node:test';

const source=readFileSync(new URL('bubbles.js',import.meta.url),'utf8');
function preview() {
  const jobs=[],closed=[],wakes=[];
  let request=0;
  const context={stampBusy:false,stampPending:false,stampWanted:{record:{}},stampGeneration:0,stamp:undefined,active:false,
    app:{bubble_stamp:()=>({request_id:++request,pending:false,size:2,alpha:new Uint8Array([0,255,255,0])})},
    rasterWorker:()=>new Promise(resolve=>jobs.push(resolve)),wake:()=>wakes.push(true),console:{error:assert.fail}};
  runInNewContext(source.slice(source.indexOf('  async function refreshStamp()'),source.indexOf('  function refresh()'))+'this.poll=refreshStamp;',context);
  const image=id=>({width:2,height:2,close:()=>closed.push(id)});
  return {context,jobs,closed,wakes,image};
}

for(const nextContact of [false,true])test(`late dab bitmap retires after contact ${nextContact?'replacement':'release'}`,async()=>{
  const h=preview(),first=h.context.poll();
  assert.equal(h.jobs.length,1);
  h.context.stampGeneration++;
  h.context.stampWanted=nextContact?{record:{}}:null;
  await h.context.poll();
  assert.equal(h.jobs.length,1,'a second contact waits for the existing worker reply');
  h.jobs.shift()(h.image('retired'));await first;
  assert.deepEqual(h.closed,['retired']);
  assert.equal(h.context.stamp,undefined,'retired pixels never become the visible preview');
  assert.equal(h.context.stampBusy,false);
  if(nextContact) {
    assert.ok(h.wakes.length,'the current contact resumes after the older reply');
    const current=h.context.poll(),image=h.image('current');
    h.jobs.shift()(image);await current;
    assert.equal(h.context.stamp.image,image);
    assert.deepEqual(h.closed,['retired']);
  }
});

test('a ready dab replaces and releases the previous bitmap',async()=>{
  const h=preview();h.context.stamp={request_id:0,image:h.image('old')};
  const ready=h.context.poll(),image=h.image('new');h.jobs.shift()(image);await ready;
  assert.equal(h.context.stamp.image,image);
  assert.equal(h.context.stampPending,false);
  assert.deepEqual(h.closed,['old']);
});
