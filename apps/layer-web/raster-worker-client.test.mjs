import test from 'node:test';
import assert from 'node:assert/strict';
import {createRasterWorker} from './raster-worker-client.js';

function fixture(module) {
  const original=Object.fromEntries(['Worker','setTimeout','clearTimeout','setInterval','clearInterval'].map(k=>[k,globalThis[k]]));
  let now=0,next=0;const timers=new Map(),workers=[];
  const schedule=(callback,delay,interval=false)=>{const id=++next;timers.set(id,{callback,at:now+delay,interval:interval?delay:0});return id;};
  globalThis.setTimeout=(f,d)=>schedule(f,d);globalThis.setInterval=(f,d)=>schedule(f,d,true);
  globalThis.clearTimeout=globalThis.clearInterval=id=>timers.delete(id);
  globalThis.Worker=class {
    constructor(){this.messages=[];this.terminated=false;workers.push(this);}
    postMessage(message,transfer){assert.equal(this.terminated,false);if(this.postError)throw this.postError;this.messages.push(message);(this.transfers??=[]).push(transfer);}
    terminate(){this.terminated=true;}
    reply(result,extra={}){this.onmessage({data:{id:this.messages.at(-1).id,result,...extra}});}
    error(){this.onerror({preventDefault(){},message:'worker failed'});}
  };
  const events=[],run=createRasterWorker(module,event=>events.push(event));
  run.configure({workers:8,requests:32,payloads:256*1024*1024,resultAge:180000,metadata:64*1024*1024,range:8*1024*1024,total:1024*1024*1024});
  return {run,workers,events,async flush(){for(let i=0;i<4;i++)await Promise.resolve();},
    async advance(delay){const end=now+delay;while(true){const ready=[...timers].filter(([,t])=>t.at<=end).sort((a,b)=>a[1].at-b[1].at)[0];if(!ready)break;const [id,t]=ready;now=t.at;if(!t.interval)timers.delete(id);t.callback();if(t.interval&&timers.has(id))t.at+=t.interval;await this.flush();}now=end;},
    close(){for(const [k,v]of Object.entries(original))globalThis[k]=v;}};
}
async function withFixture(body){const f=fixture();try{await body(f);}finally{f.close();}}
const request=(operation='snapshot',extra={})=>({operation,metadata:'{}',buffers:[],...(operation==='read'?{file:new Blob(['input'])}:{}),...extra});
const transfer=(kind='artwork',metadata='descriptor',buffers=[new Uint8Array([3,7,11])])=>({transfer_kind:kind,metadata,buffers});
const admission=(run,values)=>run.configure({metadata:64*1024*1024,range:8*1024*1024,total:1024*1024*1024,workers:8,requests:32,payloads:256*1024*1024,resultAge:180000,...values});

test('pending requests and unconsumed results share one job slot',()=>withFixture(async f=>{
  admission(f.run,{requests:1});
  const first=f.run(request('encode'));
  await assert.rejects(f.run(request()),/Raster workers are busy/);assert.equal(f.workers.length,1);
  const bytes=new Uint8Array([3,7]);f.workers[0].reply(bytes);const key=await first;
  await assert.rejects(f.run(request()),/Raster workers are busy/);
  assert.equal(f.run.consumeResult('encode',key),bytes);
  const retry=f.run(request());f.workers[1].reply('ready');assert.equal(await retry,'ready');
}));

test('aggregate payload charges survive transfer and resize to the held result',()=>withFixture(async f=>{
  admission(f.run,{payloads:20});
  const first=f.run(request('encode',{buffers:[new Uint8Array(12)]}));
  await assert.rejects(f.run(request('encode',{buffers:[new Uint8Array(1)]})),/Raster workers are busy/);
  f.workers[0].reply(new Uint8Array(17));const key=await first;
  await assert.rejects(f.run(request()),/Raster workers are busy/);
  f.run.consumeResult('encode',key);
  const retry=f.run(request('encode'));f.workers[0].reply(new Uint8Array(20));const next=await retry;
  assert.equal(f.run.consumeResult('encode',next).byteLength,20);
}));

test('a small view cannot hide a large transferred ArrayBuffer',()=>withFixture(async f=>{
  admission(f.run,{payloads:20});
  await assert.rejects(f.run(request('encode',{buffers:[new Uint8Array(32).subarray(0,1)]})),/Raster workers are busy/);
  assert.equal(f.workers.length,0);
}));

test('oversized results and transport failures return their admission',()=>withFixture(async f=>{
  admission(f.run,{requests:1,payloads:20});
  const first=f.run(request('encode')),refused=assert.rejects(first,/aggregate admission/);
  f.workers[0].reply(new Uint8Array(21));await refused;
  f.workers[0].postError=Error('transport stopped');
  await assert.rejects(f.run(request('encode')),/transport stopped/);
  delete f.workers[0].postError;
  const retry=f.run(request('encode'));f.workers[0].reply(new Uint8Array(3));const key=await retry;
  assert.equal(f.run.consumeResult('encode',key).byteLength,3);
}));

test('cancellation returns one admission slot and late replies cannot release another',()=>withFixture(async f=>{
  admission(f.run,{requests:1});let cancelled=false;
  const first=f.run(request('snapshot',{cancelled:()=>cancelled})),refused=assert.rejects(first,{name:'AbortError'}),old=f.workers[0];
  cancelled=true;await f.advance(50);await refused;
  const next=f.run(request()),current=f.workers[1];old.reply('late');
  await assert.rejects(f.run(request()),/Raster workers are busy/);
  current.reply('ready');assert.equal(await next,'ready');
}));

test('producer and independent validation retain one logical admission through consumption',()=>withFixture(async f=>{
  admission(f.run,{requests:1,workers:3});
  const pending=f.run(request('read'));f.workers[0].reply(transfer());await f.flush();
  assert.equal(f.workers.length,2);await assert.rejects(f.run(request()),/Raster workers are busy/);
  f.workers[1].reply(transfer());const result=await pending;
  await assert.rejects(f.run(request()),/Raster workers are busy/);
  assert.ok(f.run.consume('artwork',result.metadata,result.buffers));
  const retry=f.run(request());f.workers.at(-1).reply('ready');assert.equal(await retry,'ready');
}));

test('worker pressure evicts an idle arena while preserving other active owners',()=>withFixture(async f=>{
  admission(f.run,{workers:3});
  const first=f.run(request()),second=f.run(request()),a=f.workers[0],b=f.workers[1];
  await assert.rejects(f.run(request()),/Raster workers are busy/);assert.equal(f.workers.length,2);
  a.reply('first');await first;
  const next=f.run(request('fingerprint'));assert.equal(a.terminated,true);assert.equal(b.terminated,false);
  f.workers[2].reply('next');assert.equal(await next,'next');b.reply('second');assert.equal(await second,'second');
}));

test('saturated jobs and worker arenas still admit output cleanup and read cancellation',()=>withFixture(async f=>{
  admission(f.run,{workers:3,requests:2});
  const first=f.run(request()),second=f.run(request());
  const cleanup=f.run(request('output-close',{metadata:'retired-output'}));
  assert.equal(f.workers.length,3);assert.equal(f.workers[2].messages[0].request.operation,'output-discard');
  f.workers[2].reply(true);assert.equal(await cleanup,true);
  assert.equal(await f.run(request('cancel-read')),true);
  f.workers[0].reply('first');f.workers[1].reply('second');await Promise.all([first,second]);
}));

test('an abandoned opaque result expires and releases both payload and job capacity',()=>withFixture(async f=>{
  admission(f.run,{requests:1,payloads:20,resultAge:100});
  const first=f.run(request('encode'));f.workers[0].reply(new Uint8Array(20));const key=await first;
  await assert.rejects(f.run(request()),/Raster workers are busy/);
  await f.advance(100);assert.equal(f.run.consumeResult('encode',key),null);
  const retry=f.run(request('encode'));f.workers[0].reply(new Uint8Array(20));const next=await retry;
  assert.equal(f.run.consumeResult('encode',next).byteLength,20);
}));

test('producer payload admission happens before an independent validator is created',()=>withFixture(async f=>{
  admission(f.run,{requests:2,payloads:20});
  const first=f.run(request('read')),refused=assert.rejects(first,/aggregate admission/);
  f.workers[0].reply(transfer('artwork','{}',[new Uint8Array(17)]));await refused;
  assert.equal(f.workers.length,1);assert.equal(f.workers[0].terminated,true);
  const retry=f.run(request());f.workers[1].reply('ready');assert.equal(await retry,'ready');
}));

test('cancelled validation disposes any receipt produced in the completion race',()=>withFixture(async f=>{
  admission(f.run,{requests:1});
  const first=f.run(request('read')),refused=assert.rejects(first,{name:'AbortError'});
  f.workers[0].reply(transfer());await f.flush();f.workers[1].reply(transfer());
  await f.run(request('cancel-read'));await refused;
  const retry=f.run(request());f.workers.at(-1).reply('ready');assert.equal(await retry,'ready');
}));

test('callbacks can repeat identical configuration while work and receipts remain live',()=>withFixture(async f=>{
  const first=f.run(request('encode'));
  assert.doesNotThrow(()=>admission(f.run,{}));
  assert.throws(()=>admission(f.run,{requests:1}),/already in use/);
  f.workers[0].reply(new Uint8Array(3));const key=await first;
  assert.doesNotThrow(()=>admission(f.run,{}));
  assert.equal(f.run.consumeResult('encode',key).byteLength,3);
}));

test('ordinary warm roles leave room for a new reader and its independent validator',()=>withFixture(async f=>{
  for(const operation of ['fingerprint','restart-begin','color-field','encode','lookup','image-decode']){
    const pending=f.run(request(operation)),worker=f.workers.at(-1);
    worker.reply(operation==='lookup'?{metadata:'{}',bytes:new Uint8Array(3)}:new Uint8Array(3));
    const result=await pending;if(['encode','lookup','image-decode'].includes(operation))f.run.consumeResult(operation,result);
  }
  assert.equal(f.workers.filter(worker=>!worker.terminated).length,6);
  const pending=f.run(request('read'));f.workers.at(-1).reply(transfer());await f.flush();
  assert.equal(f.workers[1].terminated,false,'Checkpoint cache cannot be evicted between restart phases');
  assert.equal(f.workers.filter(worker=>!worker.terminated).length,7);
  f.workers.at(-1).reply(transfer());const result=await pending;
  assert.ok(f.run.consume('artwork',result.metadata,result.buffers));
}));

test('the cleanup lane coalesces tokens and refuses oversized or excess control inputs',()=>withFixture(async f=>{
  const first=f.run(request('output-close',{metadata:'first'})),same=f.run(request('output-close',{metadata:'first'}));
  const second=f.run(request('output-close',{metadata:'second'}));
  assert.equal(f.workers.length,1);assert.equal(f.workers[0].messages.length,2);
  await assert.rejects(f.run(request('output-close',{metadata:'third'})),/Finish the current output/);
  await assert.rejects(f.run(request('cancel-read',{buffers:[new Uint8Array(1)]})),/Invalid worker cleanup/);
  await assert.rejects(f.run(request('output-close',{metadata:'x'.repeat(129)})),/Invalid worker cleanup/);
  const worker=f.workers[0];
  worker.onmessage({data:{id:worker.messages[0].id,result:true}});assert.equal(await first,true);assert.equal(await same,true);
  worker.reply(true);assert.equal(await second,true);
}));

for(const [operation,parallel]of [['color-convert','fingerprint'],['restart-write','restart-retain']])test(`${operation} stays active while a parallel retired reply finishes before validation`,()=>withFixture(async f=>{
  admission(f.run,{workers:4});
  const first=f.run(request(operation)),second=f.run(request(parallel)),parent=f.workers[0];
  parent.onmessage({data:{id:parent.messages[0].id,result:transfer(),retire:true}});await f.flush();
  const validator=f.workers[1];parent.reply('parallel',{retire:true});assert.equal(await second,'parallel');
  await f.advance(6000);assert.equal(parent.terminated,false);assert.equal(validator.terminated,false);
  const analysis=f.run(request());await assert.rejects(f.run(request('color-field')),/Raster workers are busy/);
  assert.equal(parent.terminated,false);assert.equal(validator.terminated,false);
  f.workers[2].reply('analysis');await analysis;
  validator.reply(transfer());const result=await first;assert.ok(f.run.consume('artwork',result.metadata,result.buffers));
  await f.advance(5000);assert.equal(parent.terminated,true);
}));

test('encoded inputs clone their Blob without a whole-input buffer transfer',()=>withFixture(async f=>{
  const file=new Blob([new Uint8Array(9*1024*1024)]);
  file.arrayBuffer=()=>{throw Error('Whole file read');};
  const pending=f.run(request('read',{file})),parent=f.workers[0];
  assert.equal(parent.messages[0].request.file,file);
  assert.deepEqual(parent.messages[0].request.buffers,[]);
  assert.deepEqual(parent.transfers[0],[]);
  parent.reply(transfer());await f.flush();f.workers[1].reply(transfer());
  const result=await pending;assert.ok(f.run.consume('artwork','descriptor',result.buffers));
  assert.equal(parent.terminated,true);
}));

test('invalid Blob inputs fail before a file worker is created',()=>withFixture(async f=>{
  for(const size of [NaN,-1,1.5,1024*1024*1024+1]) {
    const file=new Blob();Object.defineProperty(file,'size',{value:size});
    await assert.rejects(f.run(request('read',{file})),/Invalid file read input/);
  }
  await assert.rejects(f.run(request('read',{file:{size:4}})),/Invalid file read input/);
  assert.equal(f.workers.length,0);
}));

test('each worker receives the shared compiled module once, including idle replacements',async()=>{
  const module=new WebAssembly.Module(new Uint8Array([0,97,115,109,1,0,0,0])),f=fixture(module);
  try{
    const first=f.run(request());assert.equal(f.workers[0].messages[0].module,module);f.workers[0].reply('first');await first;
    const repeated=f.run(request());assert.equal(f.workers[0].messages[1].module,undefined);f.workers[0].reply('repeat');await repeated;
    await f.advance(5000);
    const next=f.run(request());assert.equal(f.workers[1].messages[0].module,module);f.workers[1].reply('next');await next;
    const storage=f.run(request('restart-store-list'));assert.equal(f.workers[2].messages[0].module,module);f.workers[2].reply([]);await storage;
  }finally{f.close();}
});

for(const kind of ['device_lost','out_of_memory','panic'])test(`${kind} reports the worker cause and rejects immediately without retiring a parallel output`,()=>withFixture(async f=>{
  const role=kind==='panic'?'runtime':'worker',message=kind==='panic'?'Pipeline used before preparation':'allocation failure';
  const output=f.run(request('output-begin'));f.workers[0].reply('output');await output;
  const pending=f.run(request()),worker=f.workers[1];
  const rejected=assert.rejects(pending,{message:`Raster worker ${kind==='panic'?'runtime':'GPU'} stopped: ${message}`});
  worker.onmessage({data:{gpu_event:{role,kind,message,wasm_bytes:1000}}});
  await rejected;
  assert.equal(worker.terminated,true);
  assert.equal(f.workers[0].terminated,false);
  assert.deepEqual(f.events[0],{role,kind,message,wasm_bytes:1000,worker:'analysis:3',operations:['snapshot']});
  worker.reply('late');
  const retry=f.run(request());f.workers[2].reply('recovered');assert.equal(await retry,'recovered');
}));

test('validation events retain diagnostics while an otherwise successful worker continues',()=>withFixture(async f=>{
  const pending=f.run(request()),worker=f.workers[0];
  worker.onmessage({data:{gpu_event:{role:'worker',kind:'validation',message:'shader details'}}});
  assert.equal(worker.terminated,false);assert.equal(f.events.length,1);
  worker.reply('result');assert.equal(await pending,'result');
}));

test('a late failure cannot detach a replacement worker with the same role',()=>withFixture(async f=>{
  const first=f.run(request('encode')),old=f.workers[0];
  const rejected=assert.rejects(first,/worker failed/);old.error();await rejected;
  const next=f.run(request('encode')),current=f.workers[1];
  const bytes=new Uint8Array([7,11]);old.error();current.reply(bytes);assert.equal(f.run.consumeResult('encode',await next),bytes);
  const last=f.run(request('encode'));assert.equal(f.workers.length,2);
  current.reply(bytes);assert.equal(f.run.consumeResult('encode',await last),bytes);
}));

test('idle retirement ignores late errors after the file worker is replaced',()=>withFixture(async f=>{
  const first=f.run(request('fingerprint')),old=f.workers[0];
  old.reply('first',{retire:true});await first;await f.advance(5000);
  assert.equal(old.terminated,true);
  const next=f.run(request('fingerprint')),current=f.workers[1];
  old.error();current.reply('next');assert.equal(await next,'next');
  const last=f.run(request('fingerprint'));assert.equal(f.workers.length,2);
  current.reply('last');assert.equal(await last,'last');
}));

test('successful idle analysis worker is reused and expires after five seconds',()=>withFixture(async f=>{
  const first=f.run(request());f.workers[0].reply('first');assert.equal(await first,'first');assert.equal(f.workers[0].terminated,false);
  await f.advance(4999);const second=f.run(request());assert.equal(f.workers.length,1);f.workers[0].reply('second');assert.equal(await second,'second');
  await f.advance(5000);assert.equal(f.workers[0].terminated,true);
  const third=f.run(request());assert.equal(f.workers.length,2);f.workers[1].reply('third');assert.equal(await third,'third');
}));

test('concurrent analysis jobs have separate ownership and retain at most one idle worker',()=>withFixture(async f=>{
  const first=f.run(request()),second=f.run(request());assert.equal(f.workers.length,2);
  f.workers[1].reply('second');assert.equal(await second,'second');assert.equal(f.workers[0].terminated,false);
  f.workers[0].reply('first');assert.equal(await first,'first');assert.equal(f.workers.filter(w=>!w.terminated).length,1);
}));

test('analysis cancellation retires only its worker and ignores late replies',()=>withFixture(async f=>{
  let cancelled=false;const first=f.run(request('snapshot',{cancelled:()=>cancelled}));const rejected=assert.rejects(first,{name:'AbortError'});
  const second=f.run(request());cancelled=true;await f.advance(50);await rejected;assert.equal(f.workers[0].terminated,true);assert.equal(f.workers[1].terminated,false);
  f.workers[0].reply('late');f.workers[1].reply('second');assert.equal(await second,'second');
  const third=f.run(request());assert.equal(f.workers.length,2);f.workers[1].reply('third');assert.equal(await third,'third');
}));

test('failed oversized and timed out analysis workers are never reused',()=>withFixture(async f=>{
  for(const reason of ['reply','crash','oversized','timeout']) {
    const index=f.workers.length;const pending=f.run(request());const worker=f.workers.at(-1);
    const result=reason==='oversized'?pending:assert.rejects(pending);
    if(reason==='reply')worker.reply(undefined,{error:'bad result'});else if(reason==='crash')worker.error();else if(reason==='oversized')worker.reply('large',{retire:true});else await f.advance(180000);
    await result;assert.equal(worker.terminated,true,reason);
    const next=f.run(request());assert.equal(f.workers.length,index+2,reason);f.workers.at(-1).reply('small',{retire:true});await next;
  }
}));

test('analysis cancellation leaves an independent output job usable',()=>withFixture(async f=>{
  const output=f.run(request('output-begin'));f.workers[0].reply('owned-output');assert.equal(await output,'owned-output');
  let cancelled=false;const analysis=f.run(request('snapshot',{cancelled:()=>cancelled}));const rejected=assert.rejects(analysis,{name:'AbortError'});cancelled=true;await f.advance(50);await rejected;assert.equal(f.workers[0].terminated,false);
  const encoded=f.run(request('output-encode',{metadata:JSON.stringify({token:'owned-output'})}));assert.equal(f.workers.length,2);assert.equal(f.workers[0].messages.at(-1).request.operation,'output-encode');f.workers[0].reply('bytes');assert.equal(await encoded,'bytes');
  const closed=f.run(request('output-close',{metadata:'owned-output'}));f.workers[0].reply(true);assert.equal(await closed,true);assert.equal(f.workers[0].terminated,true);
}));

test('failed postMessage preserves its error and never retires another active job',()=>withFixture(async f=>{
  const warm=f.run(request());f.workers[0].reply('warm');await warm;
  const error=new Error('transfer rejected');f.workers[0].postError=error;
  await assert.rejects(f.run(request()),e=>e===error);assert.equal(f.workers[0].terminated,true);
  const active=f.run(request());const failed=f.run(request());const rejection=assert.rejects(failed,{message:'worker failed'});
  f.workers[2].error();await rejection;assert.equal(f.workers[1].terminated,false);
  f.workers[1].reply('active');assert.equal(await active,'active');
}));

test('already cancelled analysis sends nothing and preserves a parallel output owner',()=>withFixture(async f=>{
  const output=f.run(request('output-begin'));f.workers[0].reply('output');await output;
  await assert.rejects(f.run(request('snapshot',{cancelled:()=>true})),{name:'AbortError'});
  assert.equal(f.workers[1].messages.length,0);assert.equal(f.workers[1].terminated,true);
  assert.equal(f.workers[0].terminated,false);
  const encoded=f.run(request('output-encode',{metadata:JSON.stringify({token:'output'})}));
  f.workers[0].reply('encoded');assert.equal(await encoded,'encoded');
  const next=f.run(request());assert.equal(f.workers.length,3);f.workers[2].reply('next');assert.equal(await next,'next');
}));


test('manual save retains its output owner until publication completes',()=>withFixture(async f=>{
  const pending=f.run(request('write')),worker=f.workers[0];worker.reply({token:'archive',blob:new Blob(['exact archive'])});
  const output=await pending;assert.equal(worker.terminated,false);assert.equal(await output.blob.text(),'exact archive');
  const closed=f.run(request('output-close',{metadata:output.token}));worker.reply(true);assert.equal(await closed,true);
  assert.deepEqual(worker.messages.map(m=>m.request.operation),['write','output-close']);assert.equal(worker.terminated,true);
}));

test('separate save outputs retain independent owners',()=>withFixture(async f=>{
  const first=f.run(request('write')),a=f.workers[0];a.reply({token:'first',blob:new Blob(['first'])});await first;
  const second=f.run(request('write')),b=f.workers[1];b.reply({token:'second',blob:new Blob(['second'])});await second;
  await assert.rejects(f.run(request('write')),/Finish the current output/);
  const closeFirst=f.run(request('output-close',{metadata:'first'}));a.reply(true);await closeFirst;
  assert.equal(a.terminated,true);assert.equal(b.terminated,false);
  const closeSecond=f.run(request('output-close',{metadata:'second'}));b.reply(true);await closeSecond;assert.equal(b.terminated,true);
}));

test('failed saves release archive workers and checkpoints reuse their cache owner',()=>withFixture(async f=>{
  const pending=f.run(request('write'));const rejected=assert.rejects(pending,/Publication failed/);
  f.workers[0].reply(undefined,{error:'Publication failed'});await rejected;assert.equal(f.workers[0].terminated,true);
  const recovery=f.run(request('restart-write'));f.workers[1].reply(true);assert.equal(await recovery,true);assert.equal(f.workers[1].terminated,false);
  const begin=f.run(request('restart-begin'));f.workers[1].reply([]);assert.deepEqual(await begin,[]);assert.equal(f.workers.length,2);
}));

for(const operation of ['image-decode','nearest-coordinates','alpha-image'])test(`${operation} failure leaves file and compression workers usable`,()=>withFixture(async f=>{
  const image=f.run(request(operation,operation==='alpha-image'?{metadata:{size:2},buffers:[new Uint8Array(4)]}:{})),file=f.run(request('fingerprint')),encode=f.run(request('encode'));
  assert.equal(f.workers.length,3);
  const refused=assert.rejects(image,{message:'image decode failed'});
  f.workers[0].reply(undefined,{error:'image decode failed'});await refused;
  const bytes=new Uint8Array([7,11]);f.workers[1].reply('file');f.workers[2].reply(bytes);
  assert.equal(await file,'file');assert.equal(f.run.consumeResult('encode',await encode),bytes);
  const retryOperation=operation==='image-decode'?'nearest-coordinates':'image-decode',retry=f.run(request(retryOperation));
  f.workers[0].reply(bytes);assert.equal(f.run.consumeResult(retryOperation,await retry),bytes);
}));

test('brush alpha transfers to image preparation and returns caller-owned bitmap',()=>withFixture(async f=>{
  const alpha=new Uint8Array([0,127,255,31]);
  const pending=f.run(request('alpha-image',{metadata:{size:2},buffers:[alpha]})),worker=f.workers[0];
  assert.equal(worker.messages[0].request.buffers[0],alpha);assert.deepEqual(worker.transfers[0],[alpha.buffer]);
  const bitmap={closed:0,close(){this.closed++;}};worker.reply(bitmap);assert.equal(await pending,bitmap);assert.equal(bitmap.closed,0);
  const bytes=new Uint8Array([3,5]),next=f.run(request('image-decode'));assert.equal(f.workers.length,1);worker.reply(bytes);assert.equal(f.run.consumeResult('image-decode',await next),bytes);
  bitmap.close();assert.equal(bitmap.closed,1);
}));

for(const [operation,kind]of [['read','artwork'],['restart-open','session']])test(`${operation} ignores a supplied validated flag and waits for its independent validator`,()=>withFixture(async f=>{
  let settled=false;
  const pending=f.run(request(operation));pending.then(()=>settled=true,()=>{});
  const original=f.workers[0],untrusted={...transfer(kind),validated:true};
  original.reply(untrusted);await f.flush();
  assert.equal(settled,false);assert.equal(f.workers.length,2);
  const validator=f.workers[1],message=validator.messages[0];
  assert.equal(message.request.operation,`validate-${kind}`);
  assert.equal(message.request.metadata,untrusted.metadata);
  assert.equal(message.request.buffers,untrusted.buffers);
  assert.deepEqual(validator.transfers[0],untrusted.buffers.map(bytes=>bytes.buffer));
  assert.equal(f.run.consume(kind,untrusted.metadata,untrusted.buffers),null);
  original.onmessage({data:{id:message.id,result:transfer(kind)}});
  validator.onmessage({data:{id:original.messages[0].id,result:transfer(kind)}});
  await f.flush();assert.equal(settled,false);
  const validated=transfer(kind);validator.reply(validated);
  const held=await pending;
  assert.equal(original.terminated,true);assert.equal(validator.terminated,false);
  assert.deepEqual(held.buffers,[]);assert.notEqual(held.buffers,validated.buffers);
  assert.deepEqual(f.run.consume(kind,held.metadata,held.buffers),{metadata:validated.metadata,buffers:validated.buffers});
}));

test('validation receipts hide private buffers and cannot be copied, forged or used by another controller',()=>withFixture(async f=>{
  const buffers=[new Uint8Array([13,17,19])],result=transfer('artwork','exact metadata',buffers);
  const pending=f.run(request('validate-artwork',{metadata:result.metadata,buffers}));
  f.workers[0].reply(result);const held=await pending;
  assert.deepEqual(held.buffers,[]);assert.deepEqual(Object.keys(held.buffers),[]);
  assert.equal(held.buffers.length,0);assert.equal(held.buffers[0],undefined);
  assert.equal(JSON.stringify(held.buffers),'[]');
  for(const forged of [[],[...held.buffers],held.buffers.slice(),structuredClone(held.buffers),Object.assign([],held.buffers),Object.create(held.buffers),new Proxy(held.buffers,{}),buffers])
    assert.equal(f.run.consume('artwork',result.metadata,forged),null);
  const other=createRasterWorker();
  assert.equal(other.consume('artwork',result.metadata,held.buffers),null);
  held.buffers.push(new Uint8Array([255]));
  const consumed=f.run.consume('artwork',result.metadata,held.buffers);
  assert.equal(consumed.buffers,buffers);assert.equal(consumed.buffers[0],buffers[0]);
  assert.deepEqual([...consumed.buffers[0]],[13,17,19]);
  assert.equal(f.run.consume('artwork',result.metadata,held.buffers),null);
}));

for(const [kind,metadata]of [['session','descriptor'],['artwork','descriptor ']])test(`receipt mismatch ${kind}/${JSON.stringify(metadata)} consumes its one-shot authority`,()=>withFixture(async f=>{
  const pending=f.run(request('validate-artwork'));
  f.workers[0].reply(transfer());const held=await pending;
  assert.throws(()=>f.run.consume(kind,metadata,held.buffers),/receipt mismatch/);
  assert.equal(f.run.consume('artwork','descriptor',held.buffers),null);
}));

test('concurrent validators accept only completion from the owner and matching request identity',()=>withFixture(async f=>{
  let firstSettled=false,secondSettled=false;
  const first=f.run(request('validate-artwork')),second=f.run(request('validate-session'));
  first.then(()=>firstSettled=true,()=>{});second.then(()=>secondSettled=true,()=>{});
  const [a,b]=f.workers;
  a.onmessage({data:{id:b.messages[0].id,result:transfer('session')}});
  b.onmessage({data:{id:a.messages[0].id,result:transfer('artwork')}});
  await f.flush();assert.equal(firstSettled,false);assert.equal(secondSettled,false);
  assert.ok(Number.isSafeInteger(a.messages[0].id));assert.ok(b.messages[0].id>a.messages[0].id);
  a.reply(transfer());const artwork=await first;
  assert.equal(secondSettled,false);
  b.reply(transfer('session'));const session=await second;
  assert.ok(f.run.consume('artwork','descriptor',artwork.buffers));
  assert.ok(f.run.consume('session','descriptor',session.buffers));
}));

for(const [name,result,reason]of [
  ['wrong transfer kind',()=>transfer('session'),/Invalid worker validation result/],
  ['oversized metadata',()=>transfer('artwork','12345'),/Oversized worker metadata/],
  ['oversized block',()=>transfer('artwork','{}',[new Uint8Array(5)]),/Invalid worker payload block/],
  ['oversized total',()=>transfer('artwork','{}',[new Uint8Array(4),new Uint8Array(3)]),/exceeds admission/],
  ['shared block',()=>transfer('artwork','{}',[new Uint8Array(new SharedArrayBuffer(1))]),/Invalid worker payload block/],
])test(`validator ${name} rejects without granting a receipt`,()=>withFixture(async f=>{
  f.run.configure({workers:8,requests:32,payloads:256*1024*1024,resultAge:180000,metadata:4,range:4,total:6});
  const pending=f.run(request('validate-artwork')),rejected=assert.rejects(pending,reason);
  const worker=f.workers[0],reply=result();worker.reply(reply);await rejected;
  assert.equal(worker.terminated,true);assert.equal(f.run.consume('artwork',reply.metadata,reply.buffers),null);
}));

for(const [name,extra,reason]of [
  ['metadata',{metadata:'12345'},/Oversized worker metadata/],
  ['block',{buffers:[new Uint8Array(5)]},/Invalid worker payload block/],
  ['total',{buffers:[new Uint8Array(4),new Uint8Array(3)]},/exceeds admission/],
  ['shared block',{buffers:[new Uint8Array(new SharedArrayBuffer(1))]},/Invalid worker payload block/],
])test(`validation input ${name} is admitted before creating an owner`,()=>withFixture(async f=>{
  f.run.configure({workers:8,requests:32,payloads:256*1024*1024,resultAge:180000,metadata:4,range:4,total:6});
  await assert.rejects(f.run(request('validate-artwork',extra)),reason);
  assert.equal(f.workers.length,0);
}));

test('cancel-read during validation rejects the original read and ignores late parent and validator replies',()=>withFixture(async f=>{
  const pending=f.run(request('read')),rejected=assert.rejects(pending,{name:'AbortError'}),parent=f.workers[0];
  parent.reply(transfer());await f.flush();const validator=f.workers[1];
  assert.equal(validator.messages[0].request.operation,'validate-artwork');
  assert.equal(await f.run(request('cancel-read')),true);await rejected;
  assert.equal(parent.terminated,true);assert.equal(validator.terminated,true);
  const late=transfer();parent.reply(late);validator.reply(late);await f.flush();
  assert.equal(f.run.consume('artwork',late.metadata,late.buffers),null);
  const retry=f.run(request('validate-artwork'));assert.equal(f.workers.length,3);
  f.workers[2].reply(transfer());const held=await retry;
  assert.ok(f.run.consume('artwork',held.metadata,held.buffers));
}));

test('cancel-read retires its validation child while independent validation remains usable',()=>withFixture(async f=>{
  const reading=f.run(request('read')),rejected=assert.rejects(reading,{name:'AbortError'});
  f.workers[0].reply(transfer());await f.flush();const child=f.workers[1];
  const independent=f.run(request('validate-session')),validator=f.workers[2];
  await f.run(request('cancel-read'));await rejected;
  assert.equal(child.terminated,true);assert.equal(validator.terminated,false);
  child.reply(transfer());validator.reply(transfer('session'));const held=await independent;
  assert.ok(f.run.consume('session','descriptor',held.buffers));
}));

test('cancelling one validator preserves another owner and ignores its late completion',()=>withFixture(async f=>{
  let cancelled=false;
  const first=f.run(request('validate-artwork',{cancelled:()=>cancelled})),rejected=assert.rejects(first,{name:'AbortError'});
  const second=f.run(request('validate-session'));
  cancelled=true;await f.advance(50);await rejected;
  assert.equal(f.workers[0].terminated,true);assert.equal(f.workers[1].terminated,false);
  f.workers[0].reply(transfer());f.workers[1].reply(transfer('session'));
  const held=await second;assert.ok(f.run.consume('session','descriptor',held.buffers));
}));

test('validation reuses one idle CPU owner and retires concurrent spare and expired owners',()=>withFixture(async f=>{
  const first=f.run(request('validate-artwork'));f.workers[0].reply(transfer());await first;
  await f.advance(4999);
  const reused=f.run(request('validate-session'));assert.equal(f.workers.length,1);
  assert.deepEqual(f.workers[0].messages.map(message=>message.request.operation),['validate-artwork','validate-session']);
  const concurrent=f.run(request('validate-artwork'));assert.equal(f.workers.length,2);
  f.workers[0].reply(transfer('session'));await reused;
  f.workers[1].reply(transfer());await concurrent;
  assert.equal(f.workers[0].terminated,false);assert.equal(f.workers[1].terminated,true);
  await f.advance(5000);assert.equal(f.workers[0].terminated,true);
  const replacement=f.run(request('validate-artwork'));assert.equal(f.workers.length,3);
  f.workers[2].reply(transfer());await replacement;
}));

test('validation job identities remain safe, unique and monotonic across owner reuse and replacement',()=>withFixture(async f=>{
  const ids=[];
  for(const operation of ['validate-artwork','validate-session','read','validate-artwork']){
    const pending=f.run(request(operation));
    const active=f.workers.find(worker=>!worker.terminated&&worker.messages.at(-1)?.id>Math.max(0,...ids));
    ids.push(active.messages.at(-1).id);active.reply(transfer(operation==='validate-session'?'session':'artwork'));
    if(operation==='read'){
      await f.flush();const validator=f.workers.find(worker=>!worker.terminated&&worker.messages.at(-1)?.id>ids.at(-1));
      ids.push(validator.messages.at(-1).id);validator.reply(transfer());
    }
    await pending;
    if(operation==='validate-session')await f.advance(5000);
  }
  assert.equal(new Set(ids).size,ids.length);
  assert.ok(ids.every((id,index)=>Number.isSafeInteger(id)&&id>0&&(index===0||id>ids[index-1])));
}));

for(const operation of ['encode','image-decode','nearest-coordinates','lookup'])test(`${operation} exposes an opaque one-shot result receipt tied to its producer lane`,()=>withFixture(async f=>{
  const bytes=new Uint8Array([23,29]),result=operation==='lookup'?{metadata:'lookup descriptor',bytes}:bytes;
  const pending=f.run(request(operation));f.workers[0].reply(result);const key=await pending;
  assert.deepEqual(key,{});assert.deepEqual(Object.keys(key),[]);assert.equal(key.bytes,undefined);
  assert.notEqual(key,result);assert.equal(JSON.stringify(key),'{}');
  for(const forged of [{},{validated:true},structuredClone(key),{...key},Object.create(key),new Proxy(key,{}),result])
    assert.equal(f.run.consumeResult(operation,forged),null);
  assert.equal(createRasterWorker().consumeResult(operation,key),null);
  key.bytes=new Uint8Array([255]);key.operation='forged';
  assert.equal(f.run.consumeResult(operation,key),result);
  assert.equal(f.run.consumeResult(operation,key),null);
  assert.equal(f.run.consume(operation,'{}',key),null);
}));

test('direct result receipt operation mismatch burns its authority',()=>withFixture(async f=>{
  const pending=f.run(request('image-decode'));f.workers[0].reply(new Uint8Array([31]));const key=await pending;
  assert.throws(()=>f.run.consumeResult('nearest-coordinates',key),/result receipt mismatch/);
  assert.equal(f.run.consumeResult('image-decode',key),null);
}));

test('untrusted file result flags and forged result objects cannot mint direct receipts',()=>withFixture(async f=>{
  const result={operation:'encode',validated:true,bytes:new Uint8Array([37])};
  const pending=f.run(request('fingerprint'));f.workers[0].reply(result);assert.equal(await pending,result);
  assert.equal(f.run.consumeResult('encode',result),null);
  assert.equal(f.run.consumeResult('lookup',result),null);
  const validation=f.run(request('validate-artwork'));f.workers[1].reply(transfer());const held=await validation;
  assert.equal(f.run.consumeResult('encode',held.buffers),null);
  assert.ok(f.run.consume('artwork','descriptor',held.buffers));
}));

test('lookup retains a separate worker owner from file operations',()=>withFixture(async f=>{
  const lookup=f.run(request('lookup')),file=f.run(request('fingerprint'));
  assert.equal(f.workers.length,2);
  const [lookupWorker,fileWorker]=f.workers;
  assert.deepEqual(lookupWorker.messages.map(message=>message.request.operation),['lookup']);
  assert.deepEqual(fileWorker.messages.map(message=>message.request.operation),['fingerprint']);
  const value={metadata:'lookup descriptor',bytes:new Uint8Array([41])};
  lookupWorker.reply(value);fileWorker.reply('fingerprint');
  assert.equal(f.run.consumeResult('lookup',await lookup),value);assert.equal(await file,'fingerprint');
  const next=f.run(request('lookup')),nextFile=f.run(request('fingerprint')),rejected=assert.rejects(next,/worker failed/);
  assert.equal(f.workers.length,2);lookupWorker.error();await rejected;
  assert.equal(fileWorker.terminated,false);fileWorker.reply('still available');assert.equal(await nextFile,'still available');
}));

for(const [operation,result,reason]of [
  ['encode',()=>new Uint8Array(7),/Invalid worker payload block/],
  ['image-decode',()=>new Uint8Array(new SharedArrayBuffer(1)),/Invalid worker payload block/],
  ['nearest-coordinates',()=>({bytes:new Uint8Array(1)}),/Invalid worker payload block/],
  ['lookup',()=>({metadata:'12345',bytes:new Uint8Array(1)}),/Oversized worker metadata/],
  ['lookup',()=>({metadata:'{}',bytes:new Uint8Array(5)}),/Invalid worker payload block/],
])test(`${operation} rejects an inadmissible direct result ${reason.source}`,()=>withFixture(async f=>{
  f.run.configure({workers:8,requests:32,payloads:256*1024*1024,resultAge:180000,metadata:4,range:4,total:6});
  const pending=f.run(request(operation)),rejected=assert.rejects(pending,reason),value=result();
  f.workers[0].reply(value);await rejected;assert.equal(f.run.consumeResult(operation,value),null);
}));

test('large direct encoder payload uses its total limit while lookup keeps the bounded-block limit',()=>withFixture(async f=>{
  f.run.configure({workers:8,requests:32,payloads:256*1024*1024,resultAge:180000,metadata:4,range:4,total:6});
  const pending=f.run(request('encode')),bytes=new Uint8Array(6);
  f.workers[0].reply(bytes);const key=await pending;assert.equal(f.run.consumeResult('encode',key),bytes);
}));

test('paper-library independently validates its paper and hides tiles behind an exact receipt',()=>withFixture(async f=>{
  const paper={id:'paper-one',width:2,height:1},tiles=[new Uint8Array([43,47])];
  const pending=f.run(request('paper-library')),parent=f.workers[0];
  parent.reply({metadata:{paper,catalog:['retained']},tiles,validated:true});await f.flush();
  assert.equal(f.workers.length,2);const validator=f.workers[1];
  assert.equal(validator.messages[0].request.operation,'validate-paper');
  assert.equal(validator.messages[0].request.metadata,JSON.stringify(paper));
  assert.equal(validator.messages[0].request.buffers,tiles);
  assert.equal(f.run.consume('paper',JSON.stringify(paper),tiles),null);
  validator.reply(transfer('paper',JSON.stringify(paper),tiles));const result=await pending;
  assert.deepEqual(result.metadata,{paper,catalog:['retained']});assert.deepEqual(result.tiles,[]);
  assert.equal(f.run.consume('paper',JSON.stringify(paper),structuredClone(result.tiles)),null);
  assert.deepEqual(f.run.consume('paper',JSON.stringify(paper),result.tiles),{metadata:JSON.stringify(paper),buffers:tiles});
  assert.equal(f.run.consume('paper',JSON.stringify(paper),result.tiles),null);
}));

test('paper validation rejects another transfer kind and leaves no paper receipt',()=>withFixture(async f=>{
  const paper={id:'paper-two'},tiles=[new Uint8Array([53])];
  const pending=f.run(request('paper-library')),rejected=assert.rejects(pending,/Invalid worker validation result/);
  f.workers[0].reply({metadata:{paper},tiles});await f.flush();f.workers[1].reply(transfer('artwork',JSON.stringify(paper),tiles));
  await rejected;assert.equal(f.workers[1].terminated,true);assert.equal(f.run.consume('paper',JSON.stringify(paper),tiles),null);
}));

for(const [operation,kind]of [['fingerprint','artwork'],['restart-begin','session'],['paper-library','paper']])test(`${kind} validation suspends a retired parent's idle timer until the child finishes`,()=>withFixture(async f=>{
  const metadata=kind==='paper'?JSON.stringify({id:'slow-paper'}):'descriptor',buffers=[new Uint8Array([59])];
  const pending=f.run(request(operation)),parent=f.workers[0];
  parent.reply(kind==='paper'?{metadata:{paper:JSON.parse(metadata)},tiles:buffers}:transfer(kind,metadata,buffers),{retire:true});
  await f.flush();const validator=f.workers[1];
  assert.equal(validator.messages[0].request.operation,`validate-${kind}`);
  await f.advance(10000);assert.equal(parent.terminated,false);assert.equal(validator.terminated,false);
  validator.reply(transfer(kind,metadata,buffers));const held=await pending;
  assert.ok(f.run.consume(kind,metadata,kind==='paper'?held.tiles:held.buffers));
  await f.advance(4999);assert.equal(parent.terminated,false);
  await f.advance(1);assert.equal(parent.terminated,true);
}));
