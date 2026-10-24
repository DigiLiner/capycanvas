import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {summarizeIntervals} from '../../tools/performance/web-bubbles.mjs';
import {createDrawing} from './new-drawing-fixture.mjs';

export async function checkHdrBubbles({call,evaluate,settle}) {
  const directory=process.env.LAYER_TEST_ARTIFACTS||'artifacts/hdr-bubbles/web';
  await mkdir(directory,{recursive:true});
  const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  const wait=async expression=>{for(let i=0;i<300;i++){if(await evaluate(expression)){await settle();return;}await pause(30);}throw Error(`Timed out: ${expression}`);};
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();};
  const shot=async name=>writeFile(`${directory}/${name}.png`,Buffer.from((await call('Page.captureScreenshot',{format:'png'})).data,'base64'));
  const packets=()=>evaluate('JSON.parse(JSON.stringify(layerApp.app.bubbles(),(_,v)=>typeof v==="bigint"?Number(v):v))');
  const packet=async id=>(await packets()).find(p=>p.view.id===id);
  const rect=selector=>evaluate(`(()=>{const n=document.querySelector(${JSON.stringify(selector)}),r=n?.getBoundingClientRect();if(!r?.width||!r.height)throw Error('Missing '+${JSON.stringify(selector)});return [r.x+r.width/2,r.y+r.height/2]})()`);
  const center=id=>rect(`.tool-bubble[data-bubble="${id}"] .bubble-wheel`);
  const stable=async id=>{let previous;for(let i=0;i<100;i++){const p=await center(id);if(previous&&p.every((v,i)=>Math.abs(v-previous[i])<.1)){await pause(150);return center(id);}previous=p;await pause(40);}throw Error('Bubble did not settle');};
  const intensity=()=>evaluate('layerApp.app.color_panel().intensity');
  const near=(a,b,label,tolerance=.03)=>assert.ok(Math.abs(a-b)<=tolerance,`${label}: ${a} != ${b}`);
  let device='mouse',point=[0,0];
  const pointer=async(type,p=point,count=1)=>{point=p;const[x,y]=p;
    if(device==='touch')await call('Input.dispatchTouchEvent',{type:{down:'touchStart',move:'touchMove',up:'touchEnd',cancel:'touchCancel'}[type],touchPoints:['up','cancel'].includes(type)?[]:[{id:41,x,y}]});
    else await call('Input.dispatchMouseEvent',{type:{down:'mousePressed',move:'mouseMoved',up:'mouseReleased'}[type],x,y,button:'left',buttons:type==='up'?0:1,clickCount:count,pointerType:device,force:type==='up'?0:.65});
  };
  const tap=async(p,count=1)=>{await pointer('down',p,count);await pause(40);await pointer('up',p,count);await settle();};
  const stroke=async points=>{await pointer('down',points[0]);for(const p of points.slice(1)){await pointer('move',p);await pause(30);}await pointer('up');await settle();await pause(200);};
  const key=async(name,code)=>{for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:name,code:name,windowsVirtualKeyCode:code});await settle();};
  const escape=async()=>{for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:'Escape',code:'Escape',windowsVirtualKeyCode:27});await settle();};
  const arcPoint=async(id,f)=>{const p=await packet(id),c=await center(id),path=p.color.intensity.path,index=Math.round(f*(path.length-1));return path[index].map((v,i)=>v+c[i]);};
  const saved=await evaluate('layerApp.app.workspace_persistence()'),touchPoints=await evaluate('navigator.maxTouchPoints');
  const fixture=structuredClone(saved);fixture.layout.bubbles={bubbles:[],rings:[],kept:[]};
  fixture.layout.header={size:'medium',zones:[[{id:1,item:{kind:'capy'}}],[{id:2,item:{kind:'tool',control:{kind:'color'}}}],[]],next_id:3};
  let lastId;
  const results={browser:await evaluate('navigator.userAgent'),contacts:[],motion:[]};
  await wait('layerApp.startupTimes.complete!==null&&layerApp.app.brush_ready()');
  await send({type:'invoke',command:'new_document'});
  await evaluate(createDrawing({width:2048,height:1536,depth:'F16'}));
  await wait('layerApp.app.brush_ready()&&layerApp.app.document_color().depth==="F16"');
  await send({type:'invoke',command:'drawing_brush'});
  await send({type:'color',action:{op:'library',action:{op:'import',name:'HDR bubble journey',swatches:Array.from({length:24},(_,i)=>['Color '+i,{space:'Srgb',rgba:[(i%6)/5,Math.floor(i/6)/3,.4,1]}])}}});
  for(let i=0;i<5;i++) {
    await send({type:'set_color',rgba:[.15+i*.12,.3,.6,1]});
    const p=await evaluate('(()=>{const r=layerApp.canvas.getBoundingClientRect();return [r.x+r.width*.65,r.y+r.height*.5]})()');
    await stroke([p,[p[0]+10,p[1]+8]]);await wait('layerApp.app.brush_ready()');
  }
  await evaluate(`window.hdrBubbleEvents=[];window.hdrBubbleListener=e=>hdrBubbleEvents.push({type:e.type,device:e.pointerType,trusted:e.isTrusted});for(const t of ['pointerdown','pointermove','pointerup'])window.addEventListener(t,hdrBubbleListener,true)`);
  try {
    for(const theme of (process.env.LAYER_BUBBLE_THEMES?.split(',')||['light','dark'])) {
      for(device of (process.env.LAYER_BUBBLE_DEVICES?.split(',')||['mouse','touch','pen'])) {
        await call('Emulation.setTouchEmulationEnabled',{enabled:device==='touch',...(device==='touch'?{maxTouchPoints:5}:{})});
        await send({type:'restore_workspace',workspace:fixture});await send({type:'set_theme',theme});
        const from=await rect('[data-header-item="2"] .header-tool');
        await tap(from);await wait('!!layerApp.state().customization.drawer');
        const to=[Math.min(700,await evaluate('innerWidth*.5')),450];
        await stroke([from,[from[0],from[1]+10],[from[0],from[1]+100],to]);
        await wait('layerApp.app.bubbles().some(p=>p.view.kind==="color")');
        assert.equal(await evaluate('layerApp.state().customization.drawer??null'),null,'source drawer closes on pickup');
        const id=(await packets()).find(p=>p.view.kind==='color').view.id;lastId=id;await stable(id);
        for(const size of [136,216,344]) {
          await send({type:'bubble',action:{type:'option',id,option:{kind:'size',size}}});await stable(id);
          const p=await packet(id);assert.ok(p.color?.intensity,'HDR bubble publishes intensity geometry');
          near(p.view.diameter,size,'requested bubble size');
          const drawing=await evaluate(`(()=>{const root=document.querySelector('.tool-bubble[data-bubble="${id}"]');return {plate:!!root.querySelector('.bubble-color-plate'),arc:!!root.querySelector('.bubble-intensity'),slider:!!root.querySelector('[role="slider"]'),nativeFace:root.querySelector('.bubble-face').tagName==='BUTTON',separateSlider:!root.querySelector('[role="slider"]').closest('button')}})()`);
          assert.deepEqual(drawing,{plate:true,arc:true,slider:true,nativeFace:true,separateSlider:true},'native face and accessible slider keep separate input ownership');
          const c=await center(id);
          const rendered=await evaluate(`(()=>{const track=document.querySelector('.tool-bubble[data-bubble="${id}"] .bubble-intensity [role="slider"]'),length=track.getTotalLength();return [0,.5,1].map(f=>{const p=track.getPointAtLength(length*f),q=new DOMPoint(p.x,p.y).matrixTransform(track.getScreenCTM());return [q.x,q.y]})})()`);
          rendered.forEach((point,index)=>point.forEach((value,axis)=>near(value-c[axis],p.color.intensity.path[index*32][axis],'rendered intensity path follows shared coordinates',.35)));
          for(const f of [0,.5,1]) {const a=await arcPoint(id,f);assert.equal(await evaluate(`layerApp.app.bubble_hit(${id},${a[0]-c[0]},${a[1]-c[1]})`),'intensity','rendered arc agrees with shared hit geometry');}
          await shot(`${theme}-${device}-${size}`);
        }
        for(const kind of ['pair','recent','palette'])await send({type:'bubble',action:{type:'option',id,option:{kind}}});
        await stable(id);
        const populated=(await packet(id)).view.beads;
        assert.ok(populated.filter(b=>b.placement.bead.kind==='recent').length>=3,'recent paint colors surround the HDR plate');
        assert.ok(populated.filter(b=>b.placement.bead.kind==='swatch').length>=12,'populated palette surrounds the HDR plate');
        await shot(`${theme}-${device}-all-beads`);
        for(const kind of ['pair','recent','palette'])await send({type:'bubble',action:{type:'option',id,option:{kind}}});
        const base=await evaluate('layerApp.app.color_panel().base');
        await stroke([await arcPoint(id,.25),await arcPoint(id,.75)]);near(await intensity(),4,'native intensity commits');
        assert.deepEqual(await evaluate('layerApp.app.color_panel().base'),base,'intensity retains picker base coordinates');
        const wheelCenter=await center(id);await tap([wheelCenter[0]+20,wheelCenter[1]-10]);near(await intensity(),4,'native wheel edit retains intensity');
        const before=await intensity();await pointer('down',await arcPoint(id,.4));await pointer('move',await arcPoint(id,.9));await escape();await pointer('up');await settle();near(await intensity(),before,'Escape restores intensity');
        if(device==='touch') {await pointer('down',await arcPoint(id,.2));await pointer('move',await arcPoint(id,.8));await pointer('cancel');await settle();near(await intensity(),before,'native touch cancellation restores intensity');}
        const reset=await arcPoint(id,.6);await tap(reset);await pause(60);await tap(reset,2);near(await intensity(),0,'native repeated arc contact resets intensity');
        await key('ArrowRight',39);near(await intensity(),.1,'focused arc supports keyboard nudging');
        await key('Home',36);near(await intensity(),0,'Home restores ordinary intensity');
        if(device==='touch') {
          await pause(450);await pointer('down',await arcPoint(id,.25));await pause(1100);
          await wait(`!!document.querySelector('.panel-context-menu:popover-open')`);
          await pointer('move',await arcPoint(id,.75));await settle();near(await intensity(),4,'held arc resumes editing instead of resetting');
          await pointer('cancel');await settle();near(await intensity(),0,'resumed contact cancellation restores its starting paint');
          assert.equal(await evaluate(`!!document.querySelector('.panel-context-menu:popover-open')`),false,'resuming dismisses the held menu');
        }
        await send({type:'invoke',command:'quick_mask'});await wait(`!layerApp.app.bubbles().find(p=>p.view.id===${id}).view.color.intensity`);
        assert.equal((await packet(id)).color??null,null,'mask editing omits HDR bubble presentation');
        assert.equal(await evaluate(`!!document.querySelector('.tool-bubble[data-bubble="${id}"] .bubble-intensity,.tool-bubble[data-bubble="${id}"] .bubble-color-plate,.tool-bubble[data-bubble="${id}"] [role=slider]')`),false,'mask editing removes HDR SVG and slider nodes');
        await send({type:'invoke',command:'quick_mask'});await wait(`!!layerApp.app.bubbles().find(p=>p.view.id===${id}).view.color.intensity`);
        assert.ok(await evaluate(`hdrBubbleEvents.some(e=>e.device===${JSON.stringify(device)}&&e.type==='pointerdown'&&e.trusted)`),`${device} journey received trusted native contact`);
        results.contacts.push({theme,device,id});
        console.log(`PASS HDR bubbles ${theme}/${device}`);
      }
      device='mouse';await call('Emulation.setTouchEmulationEnabled',{enabled:false});
      const id=lastId;
      await send({type:'bubble',action:{type:'option',id,option:{kind:'size',size:216}}});await stable(id);
      for(const kind of (process.env.LAYER_HDR_BUBBLE_MOTION==='off'?[]:['drag','intensity']))for(let run=-1;run<3;run++) {
        const c=await stable(id),path=(await packet(id)).color.intensity.path,at=f=>{const n=f*(path.length-1),i=Math.min(path.length-2,Math.floor(n)),t=n-i;return path[i].map((v,k)=>v+(path[i+1][k]-v)*t+c[k]);},start=kind==='drag'?[c[0]-104,c[1]]:at(.5);
        await pointer('down',start);await pointer('move',kind==='drag'?[start[0]+14,start[1]]:at(.51));
        await evaluate(`window.hdrBubbleMotion={times:[],values:[],active:true};(()=>{const p=hdrBubbleMotion,root=document.querySelector('.tool-bubble[data-bubble="${id}"]');function tick(t){if(!p.active)return;const value=${kind==='drag'?"root.style.transform":"root.querySelector('[role=slider]')?.getAttribute('aria-valuenow')"};if(p.values.at(-1)!==value){p.times.push(t);p.values.push(value)}p.frame=requestAnimationFrame(tick)}p.frame=requestAnimationFrame(tick)})()`);
        const begin=await evaluate('({performance_ms:performance.now(),epoch_ms:Date.now()})'),hostBegin=performance.now(),duration=run<0?1000:6000;
        while(performance.now()-hostBegin<duration) {
          const t=(performance.now()-hostBegin)/1000,triangle=1-Math.abs((t/1.5)%2-1);
          await pointer('move',kind==='drag'?[start[0]+60+45*Math.sin(t*2),start[1]+30*Math.sin(t*2.7)]:at(.15+.7*triangle));
          await pause(4);
        }
        const data=await evaluate('(()=>{const p=hdrBubbleMotion;p.active=false;cancelAnimationFrame(p.frame);return {end_ms:performance.now(),times:p.times,values:p.values}})()');
        await pointer('up');await settle();
        assert.ok(new Set(data.values).size>20,`${kind} actually moves during measured contact`);
        if(run>=0)results.motion.push({theme,kind,run:run+1,begin,...data,...summarizeIntervals(data.times),measurement:'Changed DOM states at requestAnimationFrame callbacks; diagnostic only, not actual presented frames',target_met:null});
      }
    }
  } catch(error) {await shot('failure');throw error;} finally {
    results.events=await evaluate('hdrBubbleEvents');
    await writeFile(`${directory}/journey.json`,JSON.stringify(results,null,2)+'\n');
    await evaluate(`for(const t of ['pointerdown','pointermove','pointerup'])window.removeEventListener(t,hdrBubbleListener,true);delete window.hdrBubbleListener`);
    await call('Emulation.setTouchEmulationEnabled',{enabled:touchPoints>0,...(touchPoints>0?{maxTouchPoints:touchPoints}:{})});
    await escape();await send({type:'restore_workspace',workspace:saved});
  }
}
