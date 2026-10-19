import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';

export async function checkVectorMilestone({call,evaluate,settle}) {
  const directory=process.env.LAYER_TEST_ARTIFACTS??'artifacts/vector-milestone/web';
  await mkdir(directory,{recursive:true});
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();};
  const invoke=command=>send({type:'invoke',command});
  const tool=tool=>send({type:'layer',action:{op:'tool',tool}});
  const state=expression=>evaluate(`(()=>{const s=layerApp.state();return ${expression}})()`);
  const screen=([x,y])=>evaluate(`(()=>{const c=layerApp.app.camera(),r=layerApp.canvas.getBoundingClientRect();return{x:r.x+(${x}*c.zoom+c.translation[0])*r.width/c.viewport[0],y:r.y+(${y}*c.zoom+c.translation[1])*r.height/c.viewport[1]}})()`);
  const pen=async(type,at)=>call('Input.dispatchMouseEvent',{type,...await screen(at),button:'left',buttons:type==='mouseReleased'?0:1,clickCount:1,pointerType:'pen',force:.6});
  const stroke=async points=>{
    await pen('mousePressed',points[0]);
    for(const p of points.slice(1)){await pen('mouseMoved',p);await settle();}
    await pen('mouseReleased',points.at(-1));await settle();
  };
  const line=(from,to,steps=20)=>Array.from({length:steps+1},(_,i)=>[from[0]+(to[0]-from[0])*i/steps,from[1]+(to[1]-from[1])*i/steps]);
  const tap=at=>stroke([at,at]);
  const click=async selector=>{
    const at=await evaluate(`(()=>{const e=${selector};if(!e)return null;e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2}})()`);
    assert.ok(at,`no element for ${selector}`);
    for(const type of ['mouseMoved','mousePressed','mouseReleased'])await call('Input.dispatchMouseEvent',{type,...at,button:'left',buttons:type==='mousePressed'?1:0,clickCount:1});
    await settle();
  };
  const byText=(scope,text)=>`[...document.querySelectorAll(${JSON.stringify(scope)})].find(e=>e.offsetParent&&e.textContent.trim()===${JSON.stringify(text)})`;
  const chord=async(key,code,windowsVirtualKeyCode,modifiers=0)=>{for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key,code,windowsVirtualKeyCode,modifiers});await settle();};
  const color=async at=>{
    const {x,y}=await screen(at);
    const shot=await call('Page.captureScreenshot',{format:'png',clip:{x:x-2,y:y-2,width:5,height:5,scale:1}});
    return evaluate(`(async()=>{const image=new Image();image.src='data:image/png;base64,${shot.data}';await image.decode();const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;const context=canvas.getContext('2d',{willReadFrequently:true});context.drawImage(image,0,0);const d=context.getImageData(0,0,canvas.width,canvas.height).data;let darkest=[255,255,255];for(let i=0;i<d.length;i+=4)if(d[i]+d[i+1]+d[i+2]<darkest[0]+darkest[1]+darkest[2])darkest=[d[i],d[i+1],d[i+2]];return darkest})()`);
  };
  const dark=([r,g,b])=>Math.max(r,g,b)<100;
  const marked=([r,g,b])=>r+g+b<600;
  const green=([r,g,b])=>g-Math.max(r,b)>40;
  const expect=async(name,at,ink,wanted=true)=>{
    const end=Date.now()+20000;let seen;
    do{seen=await color(at);if(ink(seen)===wanted)return;await new Promise(r=>setTimeout(r,100));}while(Date.now()<end);
    assert.equal(ink(seen),wanted,`${name}: ${seen}`);
  };
  const until=async(name,expression)=>{
    const end=Date.now()+20000;
    while(!(await state(expression))){assert.ok(Date.now()<end,`${name} timed out`);await new Promise(r=>setTimeout(r,100));}
  };
  const capture=async name=>writeFile(`${directory}/${name}.png`,Buffer.from((await call('Page.captureScreenshot',{format:'png'})).data,'base64'));
  const brush=label=>evaluate(`layerApp.app.catalog().brush_categories.flatMap(c=>c.brushes).find(b=>b.label===${JSON.stringify(label)}).id`);
  const activeVector='s.layers.find(l=>l.selected)?.content_icon==="layer-path-symbolic"';
  const noticeAction=async id=>{
    await until(`notice ${id}`,`(s.notice?.actions??[]).some(a=>a.id===${JSON.stringify(id)})`);
    const label=await state(`s.notice.actions.find(a=>a.id===${JSON.stringify(id)}).label`);
    await click(byText('.canvas-notice-action',label));
  };
  const [pencil,gpen]=[await brush('Pencil'),await brush('G-Pen')];
  for(const theme of ['light','dark']){
    await send({type:'set_theme',theme});
    await send({type:'layer',action:{op:'new',group:false,clipped:false}});
    await invoke('fit_canvas');
    await send({type:'set_color',rgba:[.1,.1,.12,1]});
    await send({type:'select_brush',id:pencil});
    await send({type:'set_brush_size',value:16});
    const label=await evaluate('layerApp.app.catalog().native_copy.layers.new_vector_layer');
    await click(`document.querySelector('[aria-label=${JSON.stringify(label)}]')`);
    await until('the footer makes a vector layer',activeVector);
    assert.equal(await state('s.brush.preset'),gpen,'a pen that draws paths is chosen');
    await send({type:'set_brush_size',value:16});
    await stroke(line([400,300],[900,330]));
    await expect(`${theme} inked line`,[650,315],dark);
    await capture(`milestone-${theme}-discover`);

    await send({type:'select_brush',id:pencil});
    await tap([650,515]);
    await noticeAction('use_vector_pen');
    await until('Use G-Pen',`s.brush.preset===${gpen}`);
    await stroke(line([400,500],[900,530]));
    await expect(`${theme} pen line`,[650,515],dark);
    await send({type:'select_brush',id:pencil});
    await tap([650,715]);
    await capture(`milestone-${theme}-wrong-brush`);
    await noticeAction('new_paint_layer');
    await until('a paint layer above',`!(${activeVector})`);
    await send({type:'set_brush_size',value:30});
    await stroke(line([400,700],[900,730]));
    await expect(`${theme} pencil on paint`,[650,715],marked);

    await click(`document.querySelector('[aria-label=${JSON.stringify(label)}]')`);
    await until('another vector layer',activeVector);
    await tool({figure:{shape:'ellipse',paint:'outline'}});
    await stroke(line([1100,300],[1400,500],8));
    await send({type:'set_color',rgba:[.15,.7,.25,1]});
    await tool({region:{fill:true,source:'visible'}});
    await tap([1250,400]);
    await expect(`${theme} filled inside the outline`,[1250,450],green);
    await tap([1600,900]);
    await expect(`${theme} filled below the lines`,[1600,960],green);
    await capture(`milestone-${theme}-color`);
    await send({type:'set_color',rgba:[.1,.1,.12,1]});
    await click(`document.querySelector('[aria-label=${JSON.stringify(label)}]')`);
    await until('a vector layer for crossing lines',activeVector);
    await invoke('pen');
    await send({type:'select_brush',id:gpen});
    await send({type:'set_brush_size',value:16});
    for(const [from,to] of [[[300,1000],[1100,1000]],[[500,850],[500,1150]],[[900,850],[900,1150]]])await stroke(line(from,to));
    await chord('a','KeyA',65,2);
    const deleteLabel=await state('s.commands.find(c=>c.id==="clear_selected").label');
    await until('Select All selects the paths',`s.commands.find(c=>c.id==="clear_selected").label!==${JSON.stringify(deleteLabel)}||s.commands.find(c=>c.id==="deselect").enabled`);
    await capture(`milestone-${theme}-select-all`);
    await chord('Escape','Escape',27);
    await invoke('eraser');
    await send({type:'set_brush_size',value:16});
    const intersection=await evaluate('layerApp.state().commands.find(c=>c.id==="erase_to_intersection").label');
    await click(`document.querySelector('[data-tool-choice=${JSON.stringify(intersection)}]')`);
    await until('Erase up to intersection','s.commands.find(c=>c.id==="erase_to_intersection").selected');
    await stroke(line([700,980],[700,1020],4));
    await expect(`${theme} span between crossings`,[780,1000],dark,false);
    await expect(`${theme} kept left part`,[400,1000],dark);
    await expect(`${theme} kept right part`,[1000,1000],dark);
    await capture(`milestone-${theme}-erase`);

    const lines=await state('Number(s.layers.find(l=>l.selected).id)');
    await send({type:'layer',action:{op:'new_below'}});
    await send({type:'layer',action:{op:'select',id:lines,mask:false}});
    const vectors=await state('s.layers.filter(l=>l.content_icon==="layer-path-symbolic").length');
    await invoke('merge_down');
    await noticeAction('merge');
    await until('the lines become pixels',`s.layers.filter(l=>l.content_icon==="layer-path-symbolic").length===${vectors-1}`);
    await expect(`${theme} merged pixels`,[400,1000],dark);
    await capture(`milestone-${theme}-merged`);
    for(const id of await state('s.layers.filter(l=>!l.paper).map(l=>Number(l.id))')){
      await send({type:'layer',action:{op:'select',id,mask:false}});
      await send({type:'layer',action:{op:'delete_selected'}});
    }
  }
}
