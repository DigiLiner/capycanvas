export function createRasterWorker(module,reportGpu=()=>{}) {
  const owners = new Map();
  let next = 0, idleAnalysis, idleValidation, limits;
  const receipts=new WeakMap(),results=new WeakMap();
  const prepared=new Set(['encode','image-decode','nearest-coordinates','lookup']);
  const identity=()=>{if(next>=Number.MAX_SAFE_INTEGER)throw Error("Worker job identities exhausted");return ++next;};
  const outputs=new Map(),reads=new Set();
  function owner(kind) {
    if (owners.has(kind)) return owners.get(kind);
    const worker = new Worker(new URL("./raster-worker.js", import.meta.url), {type:"module"});
    const pending = new Map();
    let closed=false;
    let idleTimer;
    const fail = error => {
      if(closed)return;
      closed=true;
      if(idleAnalysis===state)idleAnalysis=null;
      for (const child of state.children) child.fail(error);
      state.children.clear();
      for (const job of pending.values()) { clearTimeout(job.timer); job.reject(error); }
      clearTimeout(idleTimer); pending.clear(); worker.terminate(); owners.delete(kind);
    };
    worker.onmessage = ({data}) => {
      if(closed)return;
      if(data.gpu_event){
        const event={...data.gpu_event,worker:kind,operations:[...pending.values()].map(job=>job.operation)};
        try{reportGpu(event);}catch{}
        if(['device_lost','out_of_memory','panic'].includes(event.kind))fail(new Error(`Raster worker ${event.kind==='panic'?'runtime':'GPU'} stopped: ${event.message}`));
        return;
      }
      const job = pending.get(data.id);
      if (!job) return;
      pending.delete(data.id); clearTimeout(job.timer);state.retire=!!data.retire;
      if (data.color_feature_error) job.reject({color_feature_error:data.color_feature_error});
      else if (data.error) job.reject(new Error(data.error)); else job.resolve(data.result);
      // Wasm heaps cannot shrink. Release an oversized idle file arena after
      // its transferred result is owned by the editor and no OPFS job is live.
      if((kind==="files"||kind==="checkpoint") && data.retire && !pending.size)state.park();
    };
    worker.onerror = event => { event.preventDefault(); fail(new Error(event.message || "Raster worker stopped")); };
    worker.onmessageerror = () => fail(new Error("Invalid raster worker response"));
    const state = {worker,pending,fail,module,children:new Set(),retire:false,get closed(){return closed;},active(){clearTimeout(idleTimer);},park(){idleTimer=setTimeout(()=>fail(new DOMException("Raster worker idle","AbortError")),5000);}}; owners.set(kind,state); return state;
  }
  function send(state,request,cancelled) {
    return new Promise((resolve,reject)=>{
      if(cancelled?.()){reject(new DOMException("Image operation cancelled","AbortError"));return;}
      const id=identity();
      state.active();
      const timer=setTimeout(()=>state.fail(new Error("Raster worker timed out")),request.operation==="encode"?30000:180000);
      const poll=cancelled?setInterval(()=>{if(cancelled())state.fail(new DOMException("Image operation cancelled","AbortError"));},50):null;
      const finish=callback=>value=>{clearInterval(poll);callback(value);};
      state.pending.set(id,{resolve:finish(resolve),reject:finish(reject),timer,operation:request.operation});
      try{state.worker.postMessage({id,request,module:state.module},(request.buffers||[]).map(bytes=>bytes.buffer));state.module=undefined;}
      catch(error){clearInterval(poll);clearTimeout(timer);state.pending.delete(id);reject(error);}
    });
  }
  function admitMetadata(metadata){
    if(!limits)throw Error('Worker input limits are unavailable');
    if(typeof metadata==='string'&&metadata.length>limits.metadata)throw Error('Oversized worker metadata');
  }
  function admitBuffers(buffers,maximum=limits.range){
    if(!Array.isArray(buffers))throw Error('Missing worker payloads');
    let total=0;
    for(const bytes of buffers){
      if(!(bytes instanceof Uint8Array)||!(bytes.buffer instanceof ArrayBuffer)||bytes.byteLength>maximum)throw Error('Invalid worker payload block');
      total+=bytes.byteLength;
      if(!Number.isSafeInteger(total)||total>limits.total)throw Error('Worker payload exceeds admission');
    }
  }
  function hold(kind,result){
    if(!result||result.transfer_kind!==kind)throw Error('Invalid worker validation result');
    admitMetadata(result.metadata);admitBuffers(result.buffers);
    const key=[];
    receipts.set(key,{kind,metadata:result.metadata,buffers:result.buffers});
    return {...result,buffers:key};
  }
  async function validate(kind,metadata,buffers,cancelled,parent){
    admitMetadata(metadata);admitBuffers(buffers);
    if(parent?.closed)throw new DOMException("Reading cancelled","AbortError");
    const state=idleValidation&&!idleValidation.closed?idleValidation:owner(`validation:${identity()}`);idleValidation=null;
    parent?.children.add(state);
    try{
      const result=await send(state,{operation:`validate-${kind}`,metadata,buffers},cancelled);
      if(cancelled?.())throw new DOMException('Validation cancelled','AbortError');
      const held=hold(kind,result);
      if(!state.closed&&!state.retire&&!idleValidation){idleValidation=state;state.park();}
      else state.fail(new DOMException('Validation finished','AbortError'));
      return held;
    }catch(error){state.fail(error);throw error;}
    finally{parent?.children.delete(state);}
  }
  async function checkedSend(state,request,cancelled){
    const result=await send(state,request,cancelled);
    if(prepared.has(request.operation)){
      if(request.operation==='lookup'){
        admitMetadata(result?.metadata);admitBuffers([result?.bytes]);
      }else admitBuffers([result],limits.total);
      const key={};results.set(key,{operation:request.operation,result});return key;
    }
    if(request.operation==='paper-library'&&result?.metadata?.paper){
      const metadata=JSON.stringify(result.metadata.paper);
      state.active();
      try{
        const checked=await validate('paper',metadata,result.tiles,cancelled,state);
        if(state.closed)throw new DOMException('Reading cancelled','AbortError');
        return {...result,metadata:{...result.metadata,paper:JSON.parse(checked.metadata)},tiles:checked.buffers};
      }finally{if(!state.closed&&state.retire&&!state.pending.size&&!state.children.size)state.park();}
    }
    if(result?.transfer_kind==='artwork'||result?.transfer_kind==='session'){
      state.active();
      try{
        const checked=await validate(result.transfer_kind,result.metadata,result.buffers,cancelled,state);
        if(state.closed)throw new DOMException("Reading cancelled","AbortError");
        return {...result,...checked};
      }finally{if(!state.closed&&state.retire&&!state.pending.size&&!state.children.size)state.park();}
    }
    return result;
  }
  const run=async ({cancelled,...request})=>{
    const op=request.operation;
    admitMetadata(request.metadata);
    if(op==='validate-artwork'||op==='validate-session'||op==='validate-paper')return validate(op.slice(9),request.metadata,request.buffers,cancelled);
    if(op==='cancel-read'){for(const state of reads)state.fail(new DOMException('Opening cancelled','AbortError'));reads.clear();return true;}
    if(op==='write'){
      if(outputs.size>=2)throw Error('Finish the current output before starting another');
      const state=owner(`archive:${identity()}`);
      try{const output=await checkedSend(state,request,cancelled);outputs.set(output.token,state);return output;}
      catch(error){state.fail(error);throw error;}
    }
    if(['restart-begin','restart-write','restart-retain'].includes(op))return checkedSend(owner('checkpoint'),request,cancelled);
    if((op==='read'||request.file!==undefined)&&(!(request.file instanceof Blob)||!Number.isSafeInteger(request.file.size)||request.file.size<0||request.file.size>limits.total))throw Error('Invalid file read input');
    if(op==='paper-library'&&request.file!==undefined){
      const state=owner(`paper-read:${identity()}`);
      try{return await checkedSend(state,request,cancelled);}finally{state.fail(new DOMException('Reading finished','AbortError'));}
    }
    if(op==='read'||op==='restart-open'){
      const state=owner(`read:${identity()}`);reads.add(state);
      try{return await checkedSend(state,request,cancelled);}finally{reads.delete(state);state.fail(new DOMException('Reading finished','AbortError'));}
    }
    if(op==='output-begin'){
      if(outputs.size>=2)throw Error('Finish the current output before starting another');
      const state=owner(`output:${identity()}`);
      try{const token=await checkedSend(state,request,cancelled);outputs.set(token,state);return token;}
      catch(e){state.fail(e);throw e;}
    }
    if(op.startsWith('output-')){
      const metadata=op==='output-encode'?JSON.parse(request.metadata):null;
      const token=metadata?.token??request.metadata,state=outputs.get(token);
      if(!state||state.closed){outputs.delete(token);if(op==='output-close')return checkedSend(owner('files'),{operation:'output-discard',metadata:token,buffers:[]});throw Error('Output job is no longer available');}
      try{return await checkedSend(state,request,cancelled);}
      finally{
        if(op==='output-close'||(op==='output-encode'&&(cancelled?.()||metadata.preview||metadata.flatten||metadata.clip))){outputs.delete(token);state.fail(new DOMException('Output finished','AbortError'));}
      }
    }
    if(op==='snapshot'){
      const state=idleAnalysis&&!idleAnalysis.closed?idleAnalysis:owner(`analysis:${identity()}`);idleAnalysis=null;
      try {
        const result=await checkedSend(state,request,cancelled);
        if(!state.closed&&!state.retire&&!idleAnalysis){idleAnalysis=state;state.park();}
        else state.fail(new DOMException('Analysis finished','AbortError'));
        return result;
      } catch(error){state.fail(error);throw error;}
    }
    return checkedSend(owner(op==='color-field'?'color-preview':op==='encode'?'codec':op==='lookup'?'lookup':(op==='image-decode'||op==='nearest-coordinates'||op==='alpha-image')?'image-preparation':'files'),request,cancelled);
  };
  run.configure=value=>{
    if(!value||!['metadata','range','total'].every(key=>Number.isSafeInteger(value[key])&&value[key]>0))throw Error('Invalid worker input limits');
    limits={metadata:value.metadata,range:value.range,total:value.total};
  };
  run.consume=(kind,metadata,key)=>{
    const receipt=receipts.get(key);
    if(!receipt)return null;
    receipts.delete(key);
    if(receipt.kind!==kind||receipt.metadata!==metadata)throw Error('Worker validation receipt mismatch');
    return {metadata:receipt.metadata,buffers:receipt.buffers};
  };
  run.consumeResult=(operation,key)=>{
    const receipt=results.get(key);
    if(!receipt)return null;
    results.delete(key);
    if(receipt.operation!==operation)throw Error('Worker result receipt mismatch');
    return receipt.result;
  };
  return run;
}
