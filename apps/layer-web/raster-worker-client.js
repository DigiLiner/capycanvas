export function createRasterWorker(module,reportGpu=()=>{},capacityReleased=()=>{}) {
  const owners = new Map();
  let next = 0, idleAnalysis, idleValidation, limits, jobs=0, decoding=0, payloadBytes=0;
  const receipts=new Map(),results=new Map();
  const prepared=new Set(['encode','image-decode','nearest-coordinates','lookup']);
  const identity=()=>{if(next>=Number.MAX_SAFE_INTEGER)throw Error("Worker job identities exhausted");return ++next;};
  const outputs=new Map(),reads=new Set(),closing=new Map();
  const busyErrors=new WeakMap(),waiters=new Set();
  const pressure=error=>busyErrors.has(error)||error?.memory_error===true;
  let capacityEpoch=0,capacityTimer,capacityNotice=false;
  let lastRelease={epoch:0},previousRelease=lastRelease;
  const sharedKind=op=>op==='color-field'?'color-preview':op==='encode'?'codec':op==='lookup'?'lookup':(op==='image-decode'||op==='nearest-coordinates'||op==='alpha-image')?'image-preparation':'files';
  function roles(request){
    const op=request.operation;
    if(op.startsWith('validate-'))return ['validation'];
    if(op==='read'||op==='restart-open'||op==='paper-library'&&request.file!==undefined)return ['new','validation'];
    if(op==='write'||op==='output-begin')return ['new'];
    if(op.startsWith('output-'))return [];
    if(op==='snapshot')return ['analysis'];
    return [op==='restart-begin'||op==='restart-write'||op==='restart-retain'?'checkpoint':sharedKind(op)];
  }
  function available(required){
    if(required.logical&&(jobs>=limits.requests||required.decoder&&decoding>=limits.decoders)||required.bytes>limits.payloads-payloadBytes)return false;
    const released=lastRelease.origin===required.origin?previousRelease.epoch:lastRelease.epoch;
    if(required.after!==undefined&&released<=required.after)return false;
    const retained=new Set();let extra=0;
    for(const role of required.roles){
      const state=role==='validation'?idleValidation:role==='analysis'?idleAnalysis:owners.get(role);
      if(state&&!state.closed)retained.add(state);else extra++;
    }
    const evictable=[...owners.values()].filter(state=>state.idle&&state.evictable&&!retained.has(state)).length;
    return owners.size-evictable+extra<=limits.workers-(required.control?0:1);
  }
  function changed(real=true,origin){
    if(lastRelease.origin!==origin){previousRelease=lastRelease;lastRelease={origin,epoch:++capacityEpoch};}
    else lastRelease.epoch=++capacityEpoch;
    if(real)capacityNotice=true;
    if(capacityTimer!==undefined)return;
    capacityTimer=setTimeout(()=>{
      capacityTimer=undefined;
      const notify=capacityNotice;capacityNotice=false;
      for(const waiter of [...waiters])waiter.check();
      if(notify)try{capacityReleased();}catch{}
    },0);
  }
  function busy(required,message='Raster workers are busy. Try again after the current operation finishes',after){
    const error=new Error(message);error.worker_busy=true;
    busyErrors.set(error,{...required,origin:required.origin??Symbol(),roles:[...required.roles],after,deadline:Date.now()+Math.min(limits.resultAge,180000)});
    return error;
  }
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
  function admitJob(required) {
    required.origin=Symbol();
    let {bytes,decoder}=required;
    if(bytes>limits.payloads)throw Error('Worker payload exceeds aggregate admission');
    if(jobs>=limits.requests||decoder&&decoding>=limits.decoders||bytes>limits.payloads-payloadBytes)throw busy(required);
    jobs++;decoding+=Number(decoder);payloadBytes+=bytes;
    let live=true,dispose,timer;
    return {
      required,blocked:false,
      resize(next){
        if(!Number.isSafeInteger(next)||next<0||next>limits.payloads)throw Error('Worker payload exceeds aggregate admission');
        required.bytes=Math.max(required.bytes,next);
        if(next>limits.payloads-(payloadBytes-bytes))throw busy(required);
        const released=next<bytes;payloadBytes+=next-bytes;bytes=next;if(released)changed(true,required.origin);
      },
      hold(cleanup){dispose=cleanup;timer=setTimeout(()=>this.release(),limits.resultAge);},
      release(real=true){if(live){live=false;clearTimeout(timer);jobs--;decoding-=Number(decoder);payloadBytes-=bytes;dispose?.();dispose=undefined;changed(real,required.origin);}},
      finish(success){if(!success||!dispose)this.release(!this.blocked);},
    };
  }
  function owner(kind,control=false,admission) {
    if (owners.has(kind)) return owners.get(kind);
    const maximum=limits.workers-(control?0:1);
    while(owners.size>=maximum){
      const idle=[...owners.values()].find(state=>state.idle&&state.evictable);
      if(!idle)throw busy(admission?.required??{logical:false,decoder:false,bytes:0,roles:[kind],control});
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
      clearTimeout(idleTimer); pending.clear(); worker.terminate(); owners.delete(kind);changed(!pressure(error),busyErrors.get(error)?.origin);
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
      else if (data.error) {
        const error=data.worker_busy===true&&data.memory_error!==true?busy(job.admission?.required??{logical:false,decoder:false,bytes:0,roles:[kind],control:true},data.error,job.epoch):new Error(data.error);
        if(data.memory_error===true)error.memory_error=true;job.reject(error);
      } else job.resolve(data.result);
    };
    worker.onerror = event => { event.preventDefault(); fail(new Error(event.message || "Raster worker stopped")); };
    worker.onmessageerror = () => fail(new Error("Invalid raster worker response"));
    const state = {worker,pending,fail,module,children:new Set(),retire:false,idle:false,busy:0,parkOnRetire:kind==='files'||kind==='checkpoint',evictable:kind!=='checkpoint'&&!kind.startsWith('output:')&&!kind.startsWith('archive:'),get closed(){return closed;},active(){this.idle=false;clearTimeout(idleTimer);},park(real=true,origin){const released=!this.idle;this.idle=true;clearTimeout(idleTimer);idleTimer=setTimeout(()=>fail(new DOMException("Raster worker idle","AbortError")),5000);if(released)changed(real,origin);}}; owners.set(kind,state); return state;
  }
  function send(state,request,cancelled,admission) {
    return new Promise((resolve,reject)=>{
      if(cancelled?.()){reject(new DOMException("Image operation cancelled","AbortError"));return;}
      const id=identity();
      state.active();
      const timer=setTimeout(()=>state.fail(new Error("Raster worker timed out")),request.operation==="encode"?30000:180000);
      const poll=cancelled?setInterval(()=>{if(cancelled())state.fail(new DOMException("Image operation cancelled","AbortError"));},50):null;
      const finish=callback=>value=>{clearInterval(poll);callback(value);};
      state.pending.set(id,{resolve:finish(resolve),reject:finish(reject),timer,operation:request.operation,admission,epoch:capacityEpoch});
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
    if(!job.required.roles.includes('validation'))job.required.roles.push('validation');
    const state=idleValidation&&!idleValidation.closed?idleValidation:owner(`validation:${identity()}`,false,job);idleValidation=null;
    parent?.children.add(state);
    try{
      const result=await send(state,{operation:`validate-${kind}`,metadata,buffers},cancelled,job);
      if(cancelled?.())throw new DOMException('Validation cancelled','AbortError');
      const held=hold(kind,result,job);
      if(!state.closed&&!state.retire&&!idleValidation){idleValidation=state;state.park();}
      else state.fail(new DOMException('Validation finished','AbortError'));
      return held;
    }catch(error){state.fail(error);throw error;}
    finally{parent?.children.delete(state);}
  }
  async function checkedSend(state,request,cancelled,job){
    state.busy++;let blocked=false;
    try{
    const result=await send(state,request,cancelled,job);
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
    }catch(error){blocked=pressure(error);throw error;}finally{
      state.busy--;
      if(!state.closed&&!state.busy&&!state.pending.size&&!state.children.size){
        if(state.evictable&&!state.idle){state.idle=true;changed(!blocked,job?.required.origin);}
        if(state.parkOnRetire&&state.retire)state.park(!blocked,job?.required.origin);
      }
    }
  }
  const dispatch=async (request,cancelled,job)=>{
    const op=request.operation;
    if(op==='validate-artwork'||op==='validate-session'||op==='validate-paper')return validate(op.slice(9),request.metadata,request.buffers,cancelled,undefined,job);
    if(op==='cancel-read'){for(const state of reads)state.fail(new DOMException('Opening cancelled','AbortError'));reads.clear();return true;}
    if(op==='write'){
      if(outputs.size>=2)throw Error('Finish the current output before starting another');
      const state=owner(`archive:${identity()}`,false,job);
      try{const output=await checkedSend(state,request,cancelled,job);outputs.set(output.token,state);return output;}
      catch(error){state.fail(error);throw error;}
    }
    if(['restart-begin','restart-write','restart-retain'].includes(op))return checkedSend(owner('checkpoint',false,job),request,cancelled,job);
    if(op==='paper-library'&&request.file!==undefined){
      const state=owner(`paper-read:${identity()}`,false,job);let failure;
      try{return await checkedSend(state,request,cancelled,job);}catch(error){failure=error;throw error;}finally{state.fail(failure??new DOMException('Reading finished','AbortError'));}
    }
    if(op==='read'||op==='restart-open'){
      const state=owner(`read:${identity()}`,false,job);reads.add(state);let failure;
      try{return await checkedSend(state,request,cancelled,job);}catch(error){failure=error;throw error;}finally{reads.delete(state);state.fail(failure??new DOMException('Reading finished','AbortError'));}
    }
    if(op==='output-begin'){
      if(outputs.size>=2)throw Error('Finish the current output before starting another');
      const state=owner(`output:${identity()}`,false,job);
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
      const state=idleAnalysis&&!idleAnalysis.closed?idleAnalysis:owner(`analysis:${identity()}`,false,job);idleAnalysis=null;
      try {
        const result=await checkedSend(state,request,cancelled,job);
        if(!state.closed&&!state.retire&&!idleAnalysis){idleAnalysis=state;state.park();}
        else state.fail(new DOMException('Analysis finished','AbortError'));
        return result;
      } catch(error){state.fail(error);throw error;}
    }
    return checkedSend(owner(sharedKind(op),false,job),request,cancelled,job);
  };
  const run=async ({cancelled,...request})=>{
    admitMetadata(request.metadata);
    if(typeof request.operation!=='string')throw Error('Missing worker operation');
    if(cancelled?.())throw new DOMException('Image operation cancelled','AbortError');
    if(request.operation==='cancel-read'||request.operation==='output-close'){
      if(typeof request.metadata!=='string'||request.metadata.length>128||(request.buffers?.length??0)!==0)throw Error('Invalid worker cleanup request');
      if(request.operation==='cancel-read')return dispatch(request,cancelled);
      if(closing.has(request.metadata))return closing.get(request.metadata);
      if(closing.size>=2)throw Error('Finish the current output before starting another');
      const pending=dispatch(request,cancelled).finally(()=>closing.delete(request.metadata));
      closing.set(request.metadata,pending);return pending;
    }
    admitBuffers(request.buffers,limits.total);
    if((request.operation==='read'||request.file!==undefined)&&(!(request.file instanceof Blob)||!Number.isSafeInteger(request.file.size)||request.file.size<0||request.file.size>limits.total))throw Error('Invalid file read input');
    const decoder=['read','restart-open'].includes(request.operation)||request.operation.startsWith('validate-')||request.operation==='paper-library'&&request.file!==undefined;
    const job=admitJob({logical:true,bytes:payloadSize(request.metadata,request.buffers),decoder,roles:roles(request),cancelled});
    let success=false;
    try{const result=await dispatch(request,cancelled,job);success=true;return result;}catch(error){job.blocked=pressure(error);throw error;}finally{job.finish(success);}
  };
  run.configure=value=>{
    const fields=['metadata','range','total','workers','requests','decoders','payloads','resultAge'];
    if(!value||!fields.every(key=>Number.isSafeInteger(value[key])&&value[key]>0)||value.workers<3)throw Error('Invalid worker input limits');
    if(limits&&fields.every(key=>limits[key]===value[key]))return;
    if(jobs||owners.size||waiters.size)throw Error('Worker input limits are already in use');
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
  run.waitForCapacity=(error,cancelled,deadline)=>{
    const required=busyErrors.get(error);
    if(!required)return Promise.reject(error);
    if(required.waiting)return required.waiting;
    if(deadline!==undefined){
      if(!Number.isSafeInteger(deadline)||deadline<0)return Promise.reject(new Error('Invalid worker capacity deadline'));
      required.deadline=Math.min(required.deadline,deadline);
    }
    if(waiters.size>=limits.requests)return Promise.reject(error);
    const checkCancelled=cancelled??required.cancelled;
    required.waiting=new Promise((resolve,reject)=>{
      let timer,poll;
      const finish=failure=>{
        if(!waiters.delete(waiter))return;
        clearTimeout(timer);clearInterval(poll);busyErrors.delete(error);
        if(failure)reject(failure);else resolve();
      };
      const waiter={check(){
        try{
          if(checkCancelled?.())return finish(new DOMException('Image operation cancelled','AbortError'));
          if(Date.now()>=required.deadline)return finish(error);
          if(available(required))finish();
        }catch(failure){finish(failure);}
      }};
      waiters.add(waiter);
      timer=setTimeout(()=>finish(error),Math.max(0,Math.min(limits.resultAge,required.deadline-Date.now())));
      if(checkCancelled)poll=setInterval(()=>waiter.check(),50);
      waiter.check();
    });
    return required.waiting;
  };
  return run;
}
