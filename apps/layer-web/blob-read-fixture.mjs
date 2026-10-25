import assert from 'node:assert/strict';

export async function checkBlobReads(evaluate) {
  await evaluate(`(()=>{
    const OriginalWorker=Worker,read=Blob.prototype.arrayBuffer,ranges=[],urls=new Set();
    window.blobReadChecks={OriginalWorker,originalRead:read,ranges,urls,read:blob=>read.call(blob)};
    Blob.prototype.arrayBuffer=function(){if(this.size>4)throw Error('Whole input Blob read');return read.call(this);};
    window.Worker=class extends OriginalWorker {
      constructor(url,options){
        if(!String(url).endsWith('/raster-worker.js')){super(url,options);return;}
        const source=String(url);
        const script="const queued=[],hold=event=>{event.stopImmediatePropagation();queued.push(event);};self.addEventListener('message',hold);const read=FileReaderSync.prototype.readAsArrayBuffer;FileReaderSync.prototype.readAsArrayBuffer=function(blob){if(blob.size>8*1024*1024)throw Error('Oversized Blob range');self.postMessage({blobReadRange:blob.size});return read.call(this,blob);};await import("+JSON.stringify(source)+");self.removeEventListener('message',hold);const receive=self.onmessage;for(const event of queued.splice(0))receive.call(self,event);";
        const code=URL.createObjectURL(new Blob([script],{type:'text/javascript'}));urls.add(code);
        super(code,options);this.code=code;
        this.addEventListener('message',({data})=>{if(Number.isSafeInteger(data.blobReadRange))ranges.push(data.blobReadRange);});
      }
      terminate(){if(this.code){URL.revokeObjectURL(this.code);urls.delete(this.code);}return super.terminate();}
    };
  })()`);
  return {
    async verify(){
      const ranges=await evaluate('blobReadChecks.ranges');
      assert.ok(ranges.length>0,'Imported data used worker range reads');
      assert.ok(ranges.every(length=>length>=0&&length<=8*1024*1024),'Every worker range stays bounded');
      return {range_count:ranges.length,maximum_range:ranges.reduce((maximum,length)=>Math.max(maximum,length),0),total_range_bytes:ranges.reduce((sum,length)=>sum+length,0)};
    },
    async close(){await evaluate(`(()=>{const checks=window.blobReadChecks;if(!checks)return;window.Worker=checks.OriginalWorker;Blob.prototype.arrayBuffer=checks.originalRead;for(const url of checks.urls)URL.revokeObjectURL(url);delete window.blobReadChecks;})()`);},
  };
}
