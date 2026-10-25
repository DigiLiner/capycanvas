import test from 'node:test';
import assert from 'node:assert/strict';
import {createDocumentStorage} from './document-storage.js';

const deferred=()=>{let resolve;const promise=new Promise(done=>resolve=done);return {promise,resolve};};
const notFound=()=>new DOMException('Missing private entry','NotFoundError');

function fixture(t){
  const saved=Object.getOwnPropertyDescriptor(globalThis,'navigator');
  const events=[],heldLocks=new Set();let failure,leaseName;
  class File{
    constructor(name){this.name=name;this.kind='file';this.bytes=new Uint8Array();this.arrayBufferCalls=0;this.getFileCalls=0;this.abortCalls=0;}
    async createWritable(){
      if(failure==='create')throw Error('create failed');
      let pending;const file=this;
      return {
        async write(bytes){if(failure==='write')throw Error('write failed');pending=bytes.slice();},
        async close(){if(failure==='close')throw Error('close failed');file.bytes=pending;},
        async abort(){file.abortCalls++;events.push(['abort',file.name]);},
      };
    }
    async getFile(){
      this.getFileCalls++;const bytes=this.bytes.slice(),file=this;
      return {size:this.size??bytes.length,async arrayBuffer(){file.arrayBufferCalls++;file.readStarted?.resolve();if(file.readGate)await file.readGate.promise;return bytes.buffer;}};
    }
  }
  class Directory{
    constructor(name){this.name=name;this.kind='directory';this.children=new Map();this.fileLookups=0;}
    async getDirectoryHandle(name,{create=false}={}){
      if(!this.children.has(name)){if(!create)throw notFound();this.children.set(name,new Directory(name));}
      const entry=this.children.get(name);assert.equal(entry.kind,'directory');return entry;
    }
    async getFileHandle(name,{create=false}={}){
      this.fileLookups++;events.push(['file',this.name,name,create]);
      if(!this.children.has(name)){if(!create)throw notFound();this.children.set(name,new File(name));}
      const entry=this.children.get(name);assert.equal(entry.kind,'file');return entry;
    }
    async removeEntry(name){
      events.push(['remove',this.name,name]);
      if(this.removeError)throw this.removeError;
      if(!this.children.delete(name))throw notFound();
    }
    async *entries(){yield* this.children;}
  }
  const root=new Directory('OPFS');
  const navigator={storage:{async getDirectory(){return root;}},locks:{request(name,options,callback){
    if(typeof options==='function'){callback=options;options={};}
    events.push(['lock',name,!!options.ifAvailable]);
    if(heldLocks.has(name)){assert.equal(options.ifAvailable,true);return Promise.resolve(callback(null));}
    heldLocks.add(name);
    if(!options.ifAvailable)leaseName=name;
    return Promise.resolve(callback({name})).finally(()=>heldLocks.delete(name));
  }}};
  Object.defineProperty(globalThis,'navigator',{configurable:true,value:navigator});
  t.after(()=>{if(saved)Object.defineProperty(globalThis,'navigator',saved);else delete globalThis.navigator;});
  const transport=createDocumentStorage();
  const run=(operation,metadata='',buffers=[])=>transport({operation,metadata,buffers});
  return {root,events,heldLocks,run,
    write:bytes=>run('tab-write','',[bytes]),read:key=>run('tab-read',key),release:key=>run('tab-release',key),
    directory(){const live=root.children.get('capy-live-tiles');assert.ok(live);return live.children.get(leaseName.slice('capy-live-tiles:'.length));},
    file(key){const file=this.directory().children.get(key);assert.ok(file);return file;},
    fail(value){failure=value;},
  };
}

test('private drawing chunks survive repeated reads and park/release cycles without sharing returned bytes',async t=>{
  const f=fixture(t),keys=[];
  for(let cycle=0;cycle<3;cycle++){
    const source=new Uint8Array([cycle,11,23,47]),expected=source.slice(),key=await f.write(source);keys.push(key);
    source.fill(255);
    const first=await f.read(key);assert.deepEqual(first,expected);first.fill(0);
    const second=await f.read(key);assert.deepEqual(second,expected);assert.notEqual(first.buffer,second.buffer);
    assert.equal(f.file(key).arrayBufferCalls,2);
    await f.release(key);assert.equal(f.directory().children.has(key),false);
    await assert.rejects(f.read(key),/owner expired/);await f.release(key);
  }
  assert.deepEqual(keys,['1','2','3']);
});

for(const [name,change]of [
  ['one modified byte',bytes=>{bytes[1]^=1;return bytes;}],
  ['same-length replacement',bytes=>new Uint8Array(bytes.length).fill(199)],
])test(`${name} rejects the private chunk after reading its exact-length file`,async t=>{
  const f=fixture(t),key=await f.write(new Uint8Array([3,5,7,11])),file=f.file(key);
  file.bytes=change(file.bytes.slice());
  await assert.rejects(f.read(key),/chunk changed/);
  assert.equal(file.arrayBufferCalls,1);
});

for(const [name,size]of [['truncation',2],['oversized replacement',1024*1024]])test(`${name} is refused before acquiring file bytes`,async t=>{
  const f=fixture(t),key=await f.write(new Uint8Array([3,5,7,11])),file=f.file(key);
  file.bytes=new Uint8Array(size);
  await assert.rejects(f.read(key),/chunk length changed/);
  assert.equal(file.arrayBufferCalls,0);assert.equal(file.getFileCalls,1);
});

test('a same-name file without private ownership cannot be read',async t=>{
  const f=fixture(t);
  await assert.rejects(f.read('999'),/owner expired/);
  const directory=f.directory(),unowned=await directory.getFileHandle('999',{create:true});unowned.bytes=new Uint8Array([13]);
  const lookups=directory.fileLookups;
  await assert.rejects(f.read('999'),/owner expired/);
  assert.equal(directory.fileLookups,lookups);assert.equal(unowned.getFileCalls,0);assert.equal(unowned.arrayBufferCalls,0);
});

test('released ownership rejects even when its filename is recreated with identical bytes',async t=>{
  const f=fixture(t),source=new Uint8Array([17,19,23]),key=await f.write(source);
  await f.release(key);
  const file=await f.directory().getFileHandle(key,{create:true});file.bytes=source.slice();
  await assert.rejects(f.read(key),/owner expired/);assert.equal(file.getFileCalls,0);
});

test('release invalidates ownership before a failed file removal',async t=>{
  const f=fixture(t),key=await f.write(new Uint8Array([29])),file=f.file(key);
  f.directory().removeError=new DOMException('Removal refused','NotAllowedError');
  await assert.rejects(f.release(key),{name:'NotAllowedError'});
  assert.equal(f.directory().children.get(key),file);
  await assert.rejects(f.read(key),/owner expired/);assert.equal(file.getFileCalls,0);
});

test('release during an in-flight read rejects its immutable file snapshot',async t=>{
  const f=fixture(t),key=await f.write(new Uint8Array([31,37,41])),file=f.file(key);
  file.readGate=deferred();file.readStarted=deferred();
  const reading=f.read(key),rejected=assert.rejects(reading,/chunk changed/);
  await file.readStarted.promise;await f.release(key);file.readGate.resolve();
  await rejected;await assert.rejects(f.read(key),/owner expired/);
});

for(const phase of ['create','write','close'])test(`${phase} failure removes the partial file and never publishes read ownership`,async t=>{
  const f=fixture(t);f.fail(phase);
  await assert.rejects(f.write(new Uint8Array([43,47])),new RegExp(`Cannot park drawing in browser storage: ${phase} failed`));
  assert.deepEqual([...f.directory().children.keys()],[]);
  assert.equal(f.events.filter(([event])=>event==='abort').length,phase==='create'?0:1);
  await assert.rejects(f.read('1'),/owner expired/);
  f.fail(undefined);const next=await f.write(new Uint8Array([53]));assert.equal(next,'2');assert.deepEqual(await f.read(next),new Uint8Array([53]));
});

test('missing backing files reject and release tolerates an already removed file',async t=>{
  const f=fixture(t),key=await f.write(new Uint8Array([59]));
  await f.directory().removeEntry(key);
  await assert.rejects(f.read(key),{name:'NotFoundError'});
  await f.release(key);await assert.rejects(f.read(key),/owner expired/);
});

test('initialization prunes abandoned owners while preserving live locks and unrelated stores',async t=>{
  const f=fixture(t),live=await f.root.getDirectoryHandle('capy-live-tiles',{create:true});
  await live.getDirectoryHandle('abandoned',{create:true});
  const active=await live.getDirectoryHandle('another-window',{create:true});f.heldLocks.add('capy-live-tiles:another-window');
  const recovery=await f.root.getDirectoryHandle('restart-storage',{create:true});
  await f.write(new Uint8Array([61]));
  assert.equal(live.children.has('abandoned'),false);assert.equal(live.children.get('another-window'),active);
  assert.equal(f.root.children.get('restart-storage'),recovery);
  assert.ok(f.heldLocks.has(`capy-live-tiles:${f.directory().name}`));
});

for(const metadata of ['../1','1/2','another-owner',''])test(`invalid owner ${JSON.stringify(metadata)} cannot reach a file handle`,async t=>{
  const f=fixture(t);await assert.rejects(f.read(metadata),/Invalid drawing cache owner/);
  assert.equal(f.directory().fileLookups,0);
});

test('invalid source chunk views are rejected before creating a file',async t=>{
  const f=fixture(t);
  for(const bytes of [new Uint8Array(),new Uint8Array(8*1024*1024+1),new Uint8Array(new SharedArrayBuffer(1)),new Uint16Array([67])])
    await assert.rejects(f.write(bytes),/Invalid drawing cache chunk/);
  assert.equal(f.directory().fileLookups,0);
});

test('two read slots bound acquisition and a failed read releases its slot to the next owner',async t=>{
  const f=fixture(t),keys=await Promise.all([71,73,79].map(value=>f.write(new Uint8Array([value]))));
  const files=keys.map(key=>f.file(key));
  for(const file of files){file.readGate=deferred();file.readStarted=deferred();}
  files[0].bytes[0]^=1;
  const first=f.read(keys[0]),rejected=assert.rejects(first,/chunk changed/),second=f.read(keys[1]),third=f.read(keys[2]);
  await Promise.all(files.slice(0,2).map(file=>file.readStarted.promise));
  assert.equal(files[2].getFileCalls,0);
  files[0].readGate.resolve();await rejected;await files[2].readStarted.promise;
  files[1].readGate.resolve();files[2].readGate.resolve();
  assert.deepEqual(await second,new Uint8Array([73]));assert.deepEqual(await third,new Uint8Array([79]));
});
