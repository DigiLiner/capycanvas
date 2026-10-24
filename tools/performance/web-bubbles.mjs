import assert from 'node:assert/strict';
import {mkdir, writeFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import {connectTab} from '../cdp.mjs';

export function summarizeIntervals(times) {
  const gaps=times.slice(1).map((t,i)=>t-times[i]).sort((a,b)=>a-b);
  const percentile=q=>gaps.length?gaps[Math.ceil(gaps.length*q)-1]:null;
  return {frames:times.length,raf_proxy_hz:times.length>1?(times.length-1)*1000/(times.at(-1)-times[0]):null,
    raf_gap_p50_ms:percentile(.5),raf_gap_p95_ms:percentile(.95),raf_gap_p99_ms:percentile(.99)};
}

export function parseSurfaceLatency(text) {
  const lines=text.trim().split('\n'),refreshNs=Number(lines.shift());
  assert(Number.isSafeInteger(refreshNs)&&refreshNs>0,'Missing SurfaceFlinger refresh period');
  const actualNs=[];
  for(const line of lines) {
    const columns=line.trim().split(/\s+/);
    assert.equal(columns.length,3,'Malformed SurfaceFlinger latency row');
    const actual=BigInt(columns[1]);
    if(actual>0n&&actual<9223372036854775807n) {
      assert(actual<=BigInt(Number.MAX_SAFE_INTEGER),'Presentation timestamp exceeds exact numeric range');
      actualNs.push(Number(actual));
    }
  }
  return {refreshNs,actualNs:[...new Set(actualNs)].sort((a,b)=>a-b)};
}

export async function measureBubbleMotion(cdp,{duration=6000,repeats=3,diagnostic=false,onBoundary=async()=>{}}={}) {
  assert(duration>=5000&&duration<=10000,'Each contact must last 5–10 seconds');
  assert(Number.isInteger(repeats)&&repeats>=(diagnostic?1:3),'At least three measured contacts required for qualification');
  const {evaluate,call,settle}=cdp,pause=ms=>new Promise(r=>setTimeout(r,ms));
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();};
  assert.equal(await evaluate("!!document.querySelector('dialog[open]')"),false,'Dismiss dialogs before measuring bubble input');
  const saved=await evaluate('layerApp.app.workspace_persistence()');
  const fixture=structuredClone(saved);
  fixture.layout.bubbles={bubbles:[],rings:[],kept:[]};
  const controls=[{kind:'command',command:'drawing_brush'},{kind:'color'},{kind:'color_picker'},{kind:'command',command:'undo'},{kind:'panel',panel:'layers'}];
  fixture.layout.header={size:'medium',zones:[[{id:1,item:{kind:'capy'}}],controls.map((control,i)=>({id:i+2,item:{kind:'tool',control}})),[]],next_id:7};
  const touch=async(type,p)=>call('Input.dispatchTouchEvent',{type,touchPoints:p?[{id:1,x:p[0],y:p[1]}]:[]});
  const center=id=>evaluate(`(()=>{const n=document.querySelector('.tool-bubble[data-bubble="${id}"] .bubble-face'),r=n.getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]})()`);
  const result={diagnostic,measurement:'requestAnimationFrame proxy; requires separate SurfaceFlinger actual-present analysis for qualification',contacts:[]};
  try {
    await send({type:'restore_workspace',workspace:fixture});
    const bounds=await evaluate(`(()=>{const r=document.querySelector('#workspace').getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height}})()`);
    assert(bounds.width>=600&&bounds.height>=400,'Five-bubble fixture requires a landscape workspace');
    const y=bounds.height*.55,left=bounds.width*.22,right=bounds.width*.78;
    const ids=[];
    for(let i=0;i<5;i++) {
      const position=[left+(right-left)*i/4,y],before=await evaluate('layerApp.app.bubbles().map(p=>p.view.id)');
      for(const phase of ['down','up'])await send({type:'bubble',action:{type:'drag',grab:{kind:'header',id:i+2},phase,position,viewport:[bounds.width,bounds.height]}});
      const fresh=await evaluate(`layerApp.app.bubbles().find(p=>!${JSON.stringify(before)}.includes(p.view.id))?.view.id`);
      assert(fresh!=null,`Header ${i+2} did not create a bubble`);ids.push(fresh);
    }
    await pause(1000);
    result.fixture=await evaluate(`JSON.parse(JSON.stringify({packets:layerApp.app.bubbles(),state:layerApp.state(),viewport:[innerWidth,innerHeight,devicePixelRatio],userAgent:navigator.userAgent},(_,v)=>typeof v==='bigint'?Number(v):v))`);
    result.bounds=bounds;
    await evaluate(`window.bubbleMotionProbe={times:[],placements:[],active:false};(()=>{const p=window.bubbleMotionProbe,nodes=[...document.querySelectorAll('.tool-bubble')];function tick(t){if(p.active){p.times.push(t);p.placements.push(nodes.map(n=>n.style.transform))}p.frame=requestAnimationFrame(tick)}p.frame=requestAnimationFrame(tick)})()`);
    for(let run=-1;run<repeats;run++) {
      await pause(600);
      const settled=await evaluate(`(${JSON.stringify(ids)}).map(id=>{const n=document.querySelector('.tool-bubble[data-bubble="'+id+'"]');const m=n.style.transform.match(/translate\\(([-.0-9]+)px, ([-.0-9]+)px\\)/);if(!m)throw Error('Missing settled placement');return [Number(m[1]),Number(m[2])]})`);
      const start=await center(ids[0]),waypoints=[start,...settled.slice(1).map(([x,y])=>[x+bounds.x,y+bounds.y]),start];
      await touch('touchStart',start);
      await touch('touchMove',[start[0]+12,start[1]]);
      const label=run<0?'warmup':`drag-${run+1}`;
      const boundaryBegin=await onBoundary(label,'begin');
      const begin=await evaluate(`(()=>{const p=bubbleMotionProbe;p.times=[];p.placements=[];p.active=true;performance.mark(${JSON.stringify(label+'-begin')});return {performance_ms:performance.now(),epoch_ms:Date.now(),time_origin_ms:performance.timeOrigin}})()`);
      const hostBegin=performance.now();
      let dispatchedInputCount=0;
      while(performance.now()-hostBegin<duration) {
        const elapsed=performance.now()-hostBegin,t=elapsed/duration;
        const segment=Math.min(waypoints.length-2,Math.floor(t*(waypoints.length-1))),fraction=t*(waypoints.length-1)-segment;
        const position=waypoints[segment].map((v,i)=>v+(waypoints[segment+1][i]-v)*fraction);
        await touch('touchMove',position);dispatchedInputCount++;
        await pause(Math.max(0,1000/120-(performance.now()-hostBegin-elapsed)));
      }
      const record=await evaluate(`(()=>{const p=bubbleMotionProbe;p.active=false;performance.mark(${JSON.stringify(label+'-end')});return {end_ms:performance.now(),times:p.times,placements:p.placements}})()`);
      const boundaryEnd=await onBoundary(label,'end');
      await touch('touchEnd');
      const distinctPlacements=record.placements[0]?.map((_,i)=>new Set(record.placements.map(p=>p[i])).size)||[];
      assert(distinctPlacements[0]>10,'Dragged bubble did not visibly move');
      assert(distinctPlacements.length===5&&distinctPlacements.slice(1).every(n=>n>1),`Every neighbor must visibly move during each contact (${label}: ${distinctPlacements.join(', ')})`);
      if(run>=0)result.contacts.push({label,begin,boundaryBegin,boundaryEnd,waypoints,distinctPlacements,...record,dispatchedInputCount,...summarizeIntervals(record.times)});
    }
    result.errors=[...cdp.errors];
    return result;
  } finally {
    await touch('touchCancel').catch(()=>{});
    await evaluate(`(()=>{const p=window.bubbleMotionProbe;if(p){cancelAnimationFrame(p.frame);delete window.bubbleMotionProbe}})()`).catch(()=>{});
    await send({type:'restore_workspace',workspace:saved});
  }
}

if(import.meta.url===pathToFileURL(process.argv[1]||'').href) {
  if(process.argv.includes('--help')) {
    console.log(`Prepared-photo workload (rAF proxies only):
  LAYER_WEB_URL=http://127.0.0.1:PORT/ LAYER_DEVICE_CDP=http://127.0.0.1:CDP_PORT LAYER_TEST_ARTIFACTS=artifacts/bubbles/RUN node tools/performance/web-bubbles.mjs --prepared-photo

Prepare a dedicated test origin with the tier photo plus an empty paint layer at Fit.
Run device commands through tools/devices/devices.py run with a current reservation.

For presentation measurements, collect the Chrome content surface's
SurfaceFlinger actual-present histories during the contact windows and align
native clocks to the returned boundary times. Poll often enough for consecutive
histories to overlap and deduplicate repeated entries. The exported
parseSurfaceLatency helper excludes empty and pending slots; frame callbacks
alone do not establish presented frame rate. See docs/performance/measuring.md.`);
  } else {
  const url=process.env.LAYER_WEB_URL;
  assert(url,'Set LAYER_WEB_URL to the dedicated tab opened by this run');
  assert(process.argv.includes('--prepared-photo'),'Prepare the tier photo + empty paint layer at Fit, then pass --prepared-photo');
  const output=process.env.LAYER_TEST_ARTIFACTS||'artifacts/bubbles/performance';
  await mkdir(output,{recursive:true});
  const cdp=await connectTab(process.env.LAYER_DEVICE_CDP||'http://127.0.0.1:9247',tab=>tab.url===url,{timeout:60000});
  try {
    await cdp.call('Runtime.enable');await cdp.call('Page.bringToFront');
    assert.equal(await cdp.evaluate('document.body.dataset.gpu'),'ready','Hardware WebGPU must be ready');
    const result=await measureBubbleMotion(cdp);
    await writeFile(`${output}/web-bubbles.json`,JSON.stringify(result,(_,v)=>typeof v==='bigint'?Number(v):v,2));
    console.log(JSON.stringify(result.contacts.map(({label,raf_proxy_hz,raf_gap_p99_ms})=>({label,raf_proxy_hz,raf_gap_p99_ms}))));
  } finally {await cdp.close();}
  }
}
