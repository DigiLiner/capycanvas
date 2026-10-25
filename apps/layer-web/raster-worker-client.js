export function createRasterWorker(module,reportGpu=()=>{}) {
  const owners = new Map();
  let next = 0, idleAnalysis, idleValidation, limits, jobs=0, decoding=0, payloadBytes=0;
  const receipts=new Map(),results=new Map();
  const prepared=new Set(['encode','image-decode','nearest-coordinates','lookup']);
  const identity=()=>{if(next>=Number.MAX_SAFE_INTEGER)throw Error("Worker job identities exhausted");return ++next;};
  const outputs=new Map(),reads=new Set(),closing=new Map();
  function payloadSize(metadata,buffers=[]) {
    let size=typeof metadata==='string'?metadata.length*2:0;
    if(!Array.isArray(buffers))throw Error('Missing worker payloads');
    for(const bytes of buffers){
      if(!(bytes instanceof Uint8Array)||!(bytes.buffer instanceof ArrayBuffer))throw Error('Invalid worker payload block');
      size+=bytes.buffer.byteLength;
      if(!Number.isSafeInteger(size))throw Error('Worker payload exceeds admission');
    }
    return size;
  }
  function admitJob(bytes,decoder) {
    if(jobs>=limits.requests||decoder&&decoding>=limits.decoders||bytes>limits.payloads-payloadBytes)throw Error('Raster workers are busy. Try again after the current operation finishes');
    jobs++;decoding+=Number(decoder);payloadBytes+=bytes;
    let live=true,dispose,timer;
    return {
      resize(next){if(!Number.isSafeInteger(next)||next<0||next>limits.payloads-(payloadBytes-bytes))throw Error('Worker payload exceeds aggregate admission');payloadBytes+=next-bytes;bytes=next;},
      hold(cleanup){dispose=cleanup;timer=setTimeout(()=>this.release(),limits.resultAge);},
      release(){if(live){live=false;clearTimeout(timer);jobs--;decoding-=Number(decoder);payloadBytes-=bytes;dispose?.();dispose=undefined;}},
      finish(success){if(!success||!dispose)this.release();},
    };
  }
  function owner(kind,control=false) {
    if (owners.has(kind)) return owners.get(kind);
    const maximum=limits.workers-(control?0:1);
    while(owners.size>=maximum){
      const idle=[...owners.values()].find(state=>state.idle&&state.evictable);
      if(!idle)throw Error('Raster workers are busy. Try again after the current operation finishes');
      idle.fail(new DOMException('Raster worker retired for new work','AbortError'));
    }
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
      else if (data.error) {const error=new Error(data.error);if(data.memory_error===true)error.memory_error=true;job.reject(error);} else job.resolve(data.result);
    };
    worker.onerror = event => { event.preventDefault(); fail(new Error(event.message || "Raster worker stopped")); };
    worker.onmessageerror = () => fail(new Error("Invalid raster worker response"));
    const state = {worker,pending,fail,module,children:new Set(),retire:false,idle:false,busy:0,parkOnRetire:kind==='files'||kind==='checkpoint',evictable:kind!=='checkpoint'&&!kind.startsWith('output:')&&!kind.startsWith('archive:'),get closed(){return closed;},active(){this.idle=false;clearTimeout(idleTimer);},park(){this.idle=true;clearTimeout(idleTimer);idleTimer=setTimeout(()=>fail(new DOMException("Raster worker idle","AbortError")),5000);}}; owners.set(kind,state); return state;
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
  function hold(kind,result,job){
    if(!result||result.transfer_kind!==kind)throw Error('Invalid worker validation result');
    admitMetadata(result.metadata);admitBuffers(result.buffers);
    const key=[];
    if(!job)throw Error('Unexpected control transfer');
    job.resize(payloadSize(result.metadata,result.buffers));job.hold(()=>receipts.delete(key));
    receipts.set(key,{kind,metadata:result.metadata,buffers:result.buffers,release:()=>job.release()});
    return {...result,buffers:key};
  }
  async function validate(kind,metadata,buffers,cancelled,parent,job){
    admitMetadata(metadata);admitBuffers(buffers);
    if(!job)throw Error('Unexpected control transfer');
    job.resize(payloadSize(metadata,buffers));
    if(parent?.closed)throw new DOMException("Reading cancelled","AbortError");
    const state=idleValidation&&!idleValidation.closed?idleValidation:owner(`validation:${identity()}`);idleValidation=null;
    parent?.children.add(state);
    try{
      const result=await send(state,{operation:`validate-${kind}`,metadata,buffers},cancelled);
      if(cancelled?.())throw new DOMException('Validation cancelled','AbortError');
      const held=hold(kind,result,job);
      if(!state.closed&&!state.retire&&!idleValidation){idleValidation=state;state.park();}
      else state.fail(new DOMException('Validation finished','AbortError'));
      return held;
    }catch(error){state.fail(error);throw error;}
    finally{parent?.children.delete(state);}
  }
  async function checkedSend(state,request,cancelled,job){
    state.busy++;
    try{
    const result=await send(state,request,cancelled);
    if(prepared.has(request.operation)){
      if(request.operation==='lookup'){
        admitMetadata(result?.metadata);admitBuffers([result?.bytes]);
      }else admitBuffers([result],limits.total);
      job.resize(request.operation==='lookup'?payloadSize(result.metadata,[result.bytes]):payloadSize(undefined,[result]));
      const key={};job.hold(()=>results.delete(key));results.set(key,{operation:request.operation,result,release:()=>job.release()});return key;
    }
    if(request.operation==='paper-library'&&result?.metadata?.paper){
      const metadata=JSON.stringify(result.metadata.paper);
      state.active();
      const checked=await validate('paper',metadata,result.tiles,cancelled,state,job);
      if(state.closed)throw new DOMException('Reading cancelled','AbortError');
      return {...result,metadata:{...result.metadata,paper:JSON.parse(checked.metadata)},tiles:checked.buffers};
    }
    if(result?.transfer_kind==='artwork'||result?.transfer_kind==='session'){
      state.active();
      const checked=await validate(result.transfer_kind,result.metadata,result.buffers,cancelled,state,job);
      if(state.closed)throw new DOMException("Reading cancelled","AbortError");
      return {...result,...checked};
    }
    return result;
    }finally{
      state.busy--;
      if(!state.closed&&!state.busy&&!state.pending.size&&!state.children.size){
        if(state.evictable)state.idle=true;
        if(state.parkOnRetire&&state.retire)state.park();
      }
    }
  }
  const dispatch=async (request,cancelled,job)=>{
    const op=request.operation;
    if(op==='validate-artwork'||op==='validate-session'||op==='validate-paper')return validate(op.slice(9),request.metadata,request.buffers,cancelled,undefined,job);
    if(op==='cancel-read'){for(const state of reads)state.fail(new DOMException('Opening cancelled','AbortError'));reads.clear();return true;}
    if(op==='write'){
      if(outputs.size>=2)throw Error('Finish the current output before starting another');
      const state=owner(`archive:${identity()}`);
      try{const output=await checkedSend(state,request,cancelled,job);outputs.set(output.token,state);return output;}
      catch(error){state.fail(error);throw error;}
    }
    if(['restart-begin','restart-write','restart-retain'].includes(op))return checkedSend(owner('checkpoint'),request,cancelled,job);
    if((op==='read'||request.file!==undefined)&&(!(request.file instanceof Blob)||!Number.isSafeInteger(request.file.size)||request.file.size<0||request.file.size>limits.total))throw Error('Invalid file read input');
    if(op==='paper-library'&&request.file!==undefined){
      const state=owner(`paper-read:${identity()}`);
      try{return await checkedSend(state,request,cancelled,job);}finally{state.fail(new DOMException('Reading finished','AbortError'));}
    }
    if(op==='read'||op==='restart-open'){
      const state=owner(`read:${identity()}`);reads.add(state);
      try{return await checkedSend(state,request,cancelled,job);}finally{reads.delete(state);state.fail(new DOMException('Reading finished','AbortError'));}
    }
    if(op==='output-begin'){
      if(outputs.size>=2)throw Error('Finish the current output before starting another');
      const state=owner(`output:${identity()}`);
      try{const token=await checkedSend(state,request,cancelled,job);outputs.set(token,state);return token;}
      catch(e){state.fail(e);throw e;}
    }
    if(op.startsWith('output-')){
      const metadata=op==='output-encode'?JSON.parse(request.metadata):null;
      const token=metadata?.token??request.metadata,state=outputs.get(token);
      if(!state||state.closed){outputs.delete(token);if(op==='output-close')return checkedSend(owner('files',true),{operation:'output-discard',metadata:token,buffers:[]},undefined,job);throw Error('Output job is no longer available');}
      try{return await checkedSend(state,request,cancelled,job);}
      finally{
        if(op==='output-close'||(op==='output-encode'&&(cancelled?.()||metadata.preview||metadata.flatten||metadata.clip))){outputs.delete(token);state.fail(new DOMException('Output finished','AbortError'));}
      }
    }
    if(op==='snapshot'){
      const state=idleAnalysis&&!idleAnalysis.closed?idleAnalysis:owner(`analysis:${identity()}`);idleAnalysis=null;
      try {
        const result=await checkedSend(state,request,cancelled,job);
        if(!state.closed&&!state.retire&&!idleAnalysis){idleAnalysis=state;state.park();}
        else state.fail(new DOMException('Analysis finished','AbortError'));
        return result;
      } catch(error){state.fail(error);throw error;}
    }
    return checkedSend(owner(op==='color-field'?'color-preview':op==='encode'?'codec':op==='lookup'?'lookup':(op==='image-decode'||op==='nearest-coordinates'||op==='alpha-image')?'image-preparation':'files'),request,cancelled,job);
  };
  const run=async ({cancelled,...request})=>{
    admitMetadata(request.metadata);
    if(request.operation==='cancel-read'||request.operation==='output-close'){
      if(typeof request.metadata!=='string'||request.metadata.length>128||(request.buffers?.length??0)!==0)throw Error('Invalid worker cleanup request');
      if(request.operation==='cancel-read')return dispatch(request,cancelled);
      if(closing.has(request.metadata))return closing.get(request.metadata);
      if(closing.size>=2)throw Error('Finish the current output before starting another');
      const pending=dispatch(request,cancelled).finally(()=>closing.delete(request.metadata));
      closing.set(request.metadata,pending);return pending;
    }
    const decoder=['read','restart-open'].includes(request.operation)||request.operation.startsWith('validate-')||request.operation==='paper-library'&&request.file!==undefined;
    const job=admitJob(payloadSize(request.metadata,request.buffers),decoder);
    let success=false;
    try{const result=await dispatch(request,cancelled,job);success=true;return result;}finally{job.finish(success);}
  };
  run.configure=value=>{
    const fields=['metadata','range','total','workers','requests','decoders','payloads','resultAge'];
    if(!value||!fields.every(key=>Number.isSafeInteger(value[key])&&value[key]>0)||value.workers<3)throw Error('Invalid worker input limits');
    if(limits&&fields.every(key=>limits[key]===value[key]))return;
    if(jobs||owners.size)throw Error('Worker input limits are already in use');
    limits={metadata:value.metadata,range:value.range,total:value.total,workers:value.workers,requests:value.requests,decoders:value.decoders,payloads:value.payloads,resultAge:value.resultAge};
  };
  run.consume=(kind,metadata,key)=>{
    const receipt=receipts.get(key);
    if(!receipt)return null;
    receipts.delete(key);
    receipt.release();
    if(receipt.kind!==kind||receipt.metadata!==metadata)throw Error('Worker validation receipt mismatch');
    return {metadata:receipt.metadata,buffers:receipt.buffers};
  };
  run.consumeResult=(operation,key)=>{
    const receipt=results.get(key);
    if(!receipt)return null;
    results.delete(key);
    receipt.release();
    if(receipt.operation!==operation)throw Error('Worker result receipt mismatch');
    return receipt.result;
  };
  return run;
}
