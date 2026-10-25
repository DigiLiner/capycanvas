// Immutable live tile chunks, separate from recovery archives and cancellable
// import/export workers. Async OPFS APIs keep the browser event loop responsive.
export function createDocumentStorage() {
  const owner=crypto.randomUUID(),lockName=id=>`capy-live-tiles:${id}`;
  let ready,lease,serial=0,reading=0;const waiters=[],chunks=new Map();
  async function directory(){return ready??=initialize().catch(error=>{ready=null;throw error;});}
  async function initialize(){
    const root=await (await navigator.storage.getDirectory()).getDirectoryHandle('capy-live-tiles',{create:true});
    // A crashed page loses its lock. Never touch chunks belonging to a live
    // window; recovery files use an entirely separate store.
    for await(const [id,entry] of root.entries())if(entry.kind==='directory'&&id!==owner){
      await navigator.locks.request(lockName(id),{ifAvailable:true},async lock=>{if(lock)await root.removeEntry(id,{recursive:true});});
    }
    await (lease??=new Promise((resolve,reject)=>navigator.locks.request(lockName(owner),()=>{resolve();return new Promise(()=>{});}).catch(reject)));
    return root.getDirectoryHandle(owner,{create:true});
  }
  return async function transport({operation,metadata,buffers}){
    const dir=await directory();
    if(operation==='tab-write'){
      const bytes=buffers[0];if(!(bytes instanceof Uint8Array)||!(bytes.buffer instanceof ArrayBuffer)||!bytes.byteLength||bytes.byteLength>8*1024*1024)throw Error('Invalid drawing cache chunk.');
      if(serial>=Number.MAX_SAFE_INTEGER)throw Error('Drawing cache identities exhausted.');
      const expected={length:bytes.byteLength,digest:await crypto.subtle.digest('SHA-256',bytes)};
      const key=String(++serial),file=await dir.getFileHandle(key,{create:true});let stream;
      try{stream=await file.createWritable();await stream.write(bytes);await stream.close();chunks.set(key,expected);return key;}
      catch(error){try{await stream?.abort();}catch{}try{await dir.removeEntry(key);}catch{}throw Error(`Cannot park drawing in browser storage: ${error.message||error}`);}
    }
    if(!/^\d+$/.test(metadata))throw Error('Invalid drawing cache owner.');
    if(operation==='tab-release'){chunks.delete(metadata);try{await dir.removeEntry(metadata);}catch(error){if(error.name!=='NotFoundError')throw error;}return;}
    if(operation==='tab-read'){
      const expected=chunks.get(metadata);if(!expected)throw Error('Drawing cache owner expired.');
      if(reading>=2)await new Promise(resolve=>waiters.push(resolve));else reading++;
      try{
        const file=await(await dir.getFileHandle(metadata)).getFile();if(file.size!==expected.length)throw Error('Drawing cache chunk length changed.');
        const bytes=new Uint8Array(await file.arrayBuffer()),digest=new Uint8Array(await crypto.subtle.digest('SHA-256',bytes));
        if(chunks.get(metadata)!==expected||!digest.every((byte,index)=>byte===new Uint8Array(expected.digest)[index]))throw Error('Drawing cache chunk changed.');
        return bytes;
      }
      finally{const next=waiters.shift();if(next)next();else reading--;}
    }
    throw Error('Unknown drawing cache operation.');
  };
}
