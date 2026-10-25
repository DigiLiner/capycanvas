import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';

export async function vectorHarness({call,evaluate,settle}, {directory='artifacts/vector-edges/web', lineSteps=16, keyDown='rawKeyDown'}={}) {
  directory=process.env.LAYER_TEST_ARTIFACTS??directory;
  await mkdir(directory,{recursive:true});
  const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();};
  const invoke=command=>send({type:'invoke',command});
  const tool=tool=>send({type:'layer',action:{op:'tool',tool}});
  const state=expression=>evaluate(`(()=>{const s=layerApp.state();return ${expression}})()`);
  const until=async(name,expression,timeout=20000)=>{
    const end=Date.now()+timeout;
    while(!(await state(expression))){if(Date.now()>end)throw Error(`${name} timed out: ${await state('JSON.stringify({notice:s.notice?.text,status:document.querySelector("#status")?.textContent})')}`);await pause(50);}
  };
  const idle=()=>until('idle','!layerApp.documents.busy()&&!s.document_file.busy&&layerApp.app.brush_ready()',60000);
  const camera=()=>evaluate('(()=>{const c=layerApp.app.camera(),r=layerApp.canvas.getBoundingClientRect();return{zoom:c.zoom,t:c.translation,v:c.viewport,r:{x:r.x,y:r.y,width:r.width,height:r.height}}})()');
  const steady=async()=>{let last='';for(let i=0;i<100;i++){const next=JSON.stringify(await camera());if(next===last)return;last=next;await settle();}};
  const screen=async([x,y])=>{const c=await camera();return{x:c.r.x+(x*c.zoom+c.t[0])*c.r.width/c.v[0],y:c.r.y+(y*c.zoom+c.t[1])*c.r.height/c.v[1]};};
  const size=()=>evaluate('(()=>{const t=layerApp.state().tabs.find(t=>t.active);return[t.width,t.height]})()');
  const line=(from,to,steps=lineSteps)=>Array.from({length:steps+1},(_,i)=>[from[0]+(to[0]-from[0])*i/steps,from[1]+(to[1]-from[1])*i/steps]);
  const mouseEvent=(type,p,pointerType,extra={})=>call('Input.dispatchMouseEvent',{type,...p,button:type==='mouseMoved'&&!extra.buttons?'none':'left',buttons:type==='mouseReleased'?0:(extra.buttons??1),clickCount:1,pointerType,force:pointerType==='pen'?.6:0,...extra});
  const draw=async(device,points,between=null)=>{
    await steady();
    const at=[];for(const p of points)at.push(await screen(p));
    if(device==='touch'){
      await call('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{...at[0],id:1}]});
      for(const p of at.slice(1)){await call('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{...p,id:1}]});await settle();}
      await call('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});await settle();return;
    }
    await mouseEvent('mousePressed',at[0],device);
    for(const [i,p] of at.slice(1).entries()){await mouseEvent('mouseMoved',p,device);await settle();if(between&&i===between.at)await between.run();}
    await mouseEvent('mouseReleased',at.at(-1),device);await settle();
  };
  const stroke=async points=>{
    const pen=async(type,at)=>call('Input.dispatchMouseEvent',{type,...await screen(at),button:'left',buttons:type==='mouseReleased'?0:1,clickCount:1,pointerType:'pen',force:.6});
    await pen('mousePressed',points[0]);
    for(const p of points.slice(1)){await pen('mouseMoved',p);await settle();}
    await pen('mouseReleased',points.at(-1));await settle();
  };
  const eraserEnd=async points=>{
    await steady();
    const at=[];for(const p of points)at.push(await screen(p));
    await evaluate(`(async()=>{const c=layerApp.canvas,capture=c.setPointerCapture;c.setPointerCapture=()=>{};try{
      const send=(type,[x,y],buttons,button)=>c.dispatchEvent(new PointerEvent(type,{pointerId:77,pointerType:'pen',isPrimary:true,clientX:x,clientY:y,buttons,button,pressure:buttons?.6:0,bubbles:true,cancelable:true}));
      const points=${JSON.stringify(at.map(p=>[p.x,p.y]))};
      send('pointerdown',points[0],32,5);
      for(const p of points.slice(1)){send('pointermove',p,32,-1);await new Promise(r=>requestAnimationFrame(r));}
      send('pointerup',points.at(-1),0,5);
    }finally{c.setPointerCapture=capture;}})()`);
    await settle();
  };
  const hover=async(device,p)=>{const at=await screen(p);for(const dx of [0,3])await call('Input.dispatchMouseEvent',{type:'mouseMoved',x:at.x+dx,y:at.y+dx,button:'none',buttons:0,pointerType:device});await settle();await pause(120);};
  const key=async(key,code,windowsVirtualKeyCode,modifiers=0)=>{for(const type of [keyDown,'keyUp'])await call('Input.dispatchKeyEvent',{type,key,code,windowsVirtualKeyCode,modifiers});await settle();};
  const chord=letter=>key(letter,`Key${letter.toUpperCase()}`,letter.toUpperCase().charCodeAt(0),2);
  const shot=async(at,radius,round=true)=>{
    const shot=await call('Page.captureScreenshot',{format:'png',clip:{x:(round?Math.round(at.x):at.x)-radius,y:(round?Math.round(at.y):at.y)-radius,width:2*radius+1,height:2*radius+1,scale:1}});
    return evaluate(`(async()=>{const image=new Image();image.src='data:image/png;base64,${shot.data}';await image.decode();const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;const context=canvas.getContext('2d',{willReadFrequently:true});context.drawImage(image,0,0);return Array.from(context.getImageData(0,0,canvas.width,canvas.height).data)})()`);
  };
  const darkest=async p=>{const d=await shot(await screen(p),4);let best=[255,255,255];for(let i=0;i<d.length;i+=4)if(d[i]+d[i+1]+d[i+2]<best[0]+best[1]+best[2])best=[d[i],d[i+1],d[i+2]];return best;};
  const color=async(at,mean=false)=>{
    const d=await shot(await screen(at),2,false);
    if(mean)return [0,1,2].map(c=>d.reduce((sum,v,i)=>sum+(i%4===c?v:0),0)/(d.length/4));
    let best=[255,255,255];
    for(let i=0;i<d.length;i+=4)if(d[i]+d[i+1]+d[i+2]<best[0]+best[1]+best[2])best=[d[i],d[i+1],d[i+2]];
    return best;
  };
  const inked=([r,g,b])=>Math.min(r,g,b)<110;
  const park=()=>call('Input.dispatchMouseEvent',{type:'mouseMoved',x:2,y:2,button:'none',buttons:0,pointerType:'mouse'});
  const expect=async(name,at,wanted=true)=>{
    await park();await settle();
    const end=Date.now()+20000;let seen;
    do{seen=await darkest(at);if(inked(seen)===wanted)return;await pause(100);}while(Date.now()<end);
    await capture(`failure-${name.replace(/[^a-z0-9]+/gi,'-')}`);
    assert.equal(inked(seen),wanted,`${name} at ${at}: ${seen} ${JSON.stringify(await camera())} ${await evaluate('JSON.stringify([innerWidth,innerHeight])')}`);
  };
  const refusalCursor=async(device,at)=>{await hover(device,at);const p=await screen(at);const d=await shot({x:p.x+3,y:p.y+3},14);let red=0;for(let i=0;i<d.length;i+=4)if(d[i]>170&&d[i+1]<90&&d[i+2]<90)red++;return red>4;};
  const capture=async name=>writeFile(`${directory}/${name}.png`,Buffer.from((await call('Page.captureScreenshot',{format:'png'})).data,'base64'));
  const click=async selector=>{
    const at=await evaluate(`(()=>{const e=${selector};if(!e)return null;e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2}})()`);
    assert.ok(at,`no element for ${selector}`);
    for(const type of ['mouseMoved','mousePressed','mouseReleased'])await call('Input.dispatchMouseEvent',{type,...at,button:'left',buttons:type==='mousePressed'?1:0,clickCount:1});
    await settle();
  };
  const noticeShape=place=>evaluate(`(()=>{const out=[],place=${JSON.stringify(place)},n=document.querySelector('.canvas-notice'),text=n.querySelector('.canvas-notice-text'),box=n.getBoundingClientRect();
    const tile=layerApp.app.toolbar_ui({type:'style',style:'small'}).size[1],control=layerApp.app.catalog().panel_metrics.control_height,clear=(tile-control)/2;
    const actions=[...n.querySelectorAll('.canvas-notice-action')].filter(b=>b.checkVisibility()),stacked=n.classList.contains('stacked');
    const range=document.createRange();range.selectNodeContents(text);const lines=new Set([...range.getClientRects()].map(r=>Math.round(r.top))).size;
    if(!stacked&&actions.length&&lines>1)out.push(place+': the text wraps to '+lines+' lines beside the actions');
    if(!stacked&&Math.abs(box.height-tile)>.5)out.push(place+': a one-row notice is '+box.height+' px tall, not the bar tile '+tile);
    const heights=actions.map(b=>b.getBoundingClientRect().height);
    if(heights.some(h=>Math.abs(h-control)>.5))out.push(place+': action heights '+heights+' differ from the worded bar control '+control);
    if(actions.length&&!actions[0].classList.contains('suggested-action'))out.push(place+': the first action is not the suggestion');
    if(actions.slice(1).some(b=>b.classList.contains('suggested-action')))out.push(place+': a later action is accented');
    if(stacked&&actions.length){
      if(actions[0].getBoundingClientRect().top<text.getBoundingClientRect().bottom-1)out.push(place+': stacked actions overlap the text');
      const end=box.right-actions.at(-1).getBoundingClientRect().right;
      if(Math.abs(end-clear)>1)out.push(place+': stacked actions end '+end+' px from the right edge');
    }
    return out;})()`);
  const brush=label=>evaluate(`layerApp.app.catalog().brush_categories.flatMap(c=>c.brushes).find(b=>b.label===${JSON.stringify(label)}).id`);
  const activeVector='s.layers.find(l=>l.selected)?.content_icon==="layer-path-symbolic"';
  const vectorLayers='s.layers.filter(l=>l.content_icon==="layer-path-symbolic").length';
  const toolIs=name=>`JSON.stringify(s.layer_tools.tool)===${JSON.stringify(JSON.stringify(name))}`;
  return {pause,send,invoke,tool,state,until,idle,camera,screen,size,line,mouseEvent,draw,stroke,color,eraserEnd,hover,key,chord,shot,darkest,inked,park,expect,refusalCursor,capture,click,noticeShape,brush,activeVector,vectorLayers,toolIs};
}
