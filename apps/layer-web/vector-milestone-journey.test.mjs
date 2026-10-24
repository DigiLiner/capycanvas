import assert from 'node:assert/strict';
import {vectorHarness} from './vector-journey-fixture.mjs';
import {createDrawing} from './new-drawing-fixture.mjs';

export async function checkVectorMilestone({call,evaluate,settle}) {
  const {send,invoke,tool,state,stroke,line,key:chord,color,capture,click,until,brush,activeVector}=await vectorHarness({call,evaluate,settle},{directory:'artifacts/vector-milestone/web',lineSteps:20,keyDown:'keyDown'});
  const tap=at=>stroke([at,at]);
  const byText=(scope,text)=>`[...document.querySelectorAll(${JSON.stringify(scope)})].find(e=>e.offsetParent&&e.textContent.trim()===${JSON.stringify(text)})`;
  const dark=([r,g,b])=>Math.max(r,g,b)<100;
  const marked=([r,g,b])=>r+g+b<600;
  const green=([r,g,b])=>g-Math.max(r,b)>40;
  const expect=async(name,at,ink,wanted=true)=>{
    const end=Date.now()+20000;let seen;
    do{seen=await color(at);if(ink(seen)===wanted)return;await new Promise(r=>setTimeout(r,100));}while(Date.now()<end);
    assert.equal(ink(seen),wanted,`${name}: ${seen}`);
  };
  const noticeAction=async id=>{
    await until(`notice ${id}`,`(s.notice?.actions??[]).some(a=>a.id===${JSON.stringify(id)})`);
    const label=await state(`s.notice.actions.find(a=>a.id===${JSON.stringify(id)}).label`);
    await click(byText('.canvas-notice-action',label));
  };
  const shown=selector=>`[...document.querySelectorAll(${JSON.stringify(selector)})].some(n=>n.checkVisibility()&&n.getBoundingClientRect().width>0)`;
  const headerTool=async match=>`document.querySelector('[data-header-item="${await state(`s.workspace.layout.header.zones.flat().find(e=>${match}).id`)}"] .header-tool')`;
  const steady=async selector=>{
    const end=Date.now()+20000;let last=null;
    for(;;){
      const now=await state(`(n=>n&&JSON.stringify(n.getBoundingClientRect()))([...document.querySelectorAll(${JSON.stringify(selector)})].find(n=>n.checkVisibility()))`);
      if(now&&now===last)return;
      assert.ok(Date.now()<end,`${selector} keeps moving`);
      last=now;await new Promise(r=>setTimeout(r,100));
    }
  };
  const inDrawer=async(match,selector,name,act)=>{
    const drawer=!(await state(shown(selector)));
    if(drawer){await click(await headerTool(match));await until(`the drawer for ${name}`,shown(selector));await steady(selector);}
    await act();
    if(drawer){await click(await headerTool(match));await until(`the drawer for ${name} closes`,`!${shown(selector)}`);}
  };
  const fromDrawer=(match,selector,name,done)=>inDrawer(match,selector,name,async()=>{
    await click(`[...document.querySelectorAll(${JSON.stringify(selector)})].find(n=>n.checkVisibility())`);
    await until(name,done);
  });
  const layersPanel='e.item.control?.panel==="layers"';
  const newVectorLayer=(label,name)=>fromDrawer(layersPanel,`.layer-footer [aria-label=${JSON.stringify(label)}]`,name,activeVector);
  const greenThumbnail=(id,name)=>inDrawer(layersPanel,`[data-layer="${id}"] .layer-thumbnail canvas`,`the thumbnail of layer ${id}`,async()=>{
    const end=Date.now()+30000;let seen;
    do{
      seen=await evaluate(`(()=>{const layer=layerApp.state().layers.find(l=>Number(l.id)===${id}),c=document.querySelector('[data-layer="${id}"] .layer-thumbnail canvas');if(!layer||!c)return null;
        const d=c.getContext('2d',{willReadFrequently:true}).getImageData(0,0,c.width,c.height).data;let green=0;for(let i=0;i<d.length;i+=4)if(d[i+3]>128&&d[i+1]-Math.max(d[i],d[i+2])>40)green++;
        return{green,revision:c.dataset.previewRevision,paint:String(layer.paint_revision)}})()`);
      if(seen?.green>0)break;
      await new Promise(r=>setTimeout(r,100));
    }while(Date.now()<end);
    await capture(`milestone-${name}-thumbnail`);
    assert.ok(seen?.green>0,`${name}: the filled layer's thumbnail shows no fill ${JSON.stringify(seen)}`);
  });
  const [pencil,gpen]=[await brush('Pencil'),await brush('G-Pen')];
  await invoke('new_document');
  await evaluate(createDrawing({width:2000,height:1400}));
  await until('the vector drawing is ready','!layerApp.documents.busy()&&!s.document_file.busy&&layerApp.app.brush_ready()',60000);
  for(const theme of ['light','dark']){
    await send({type:'set_theme',theme});
    await send({type:'layer',action:{op:'new',group:false,clipped:false}});
    await invoke('fit_canvas');
    await send({type:'set_color',rgba:[.1,.1,.12,1]});
    await send({type:'select_brush',id:pencil});
    await send({type:'set_brush_size',value:16});
    const label=await evaluate('layerApp.app.catalog().native_copy.layers.new_vector_layer');
    await newVectorLayer(label,'the footer makes a vector layer');
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

    await newVectorLayer(label,'another vector layer');
    await tool({figure:{shape:'ellipse',paint:'outline'}});
    await stroke(line([1100,300],[1400,500],8));
    await send({type:'set_color',rgba:[.15,.7,.25,1]});
    await tool({region:{fill:true,source:'visible'}});
    await tap([1250,400]);
    await expect(`${theme} filled inside the outline`,[1250,450],green);
    const unfilled=await state('s.layers.map(l=>Number(l.id))');
    await tap([1600,900]);
    await expect(`${theme} filled below the lines`,[1600,960],green);
    await greenThumbnail(await state(`Number(s.layers.find(l=>!${JSON.stringify(unfilled)}.includes(Number(l.id))).id)`),`${theme}-fill-below`);
    await capture(`milestone-${theme}-color`);
    await send({type:'set_color',rgba:[.1,.1,.12,1]});
    await newVectorLayer(label,'a vector layer for crossing lines');
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
    await fromDrawer('e.item.control?.command==="eraser"',`[data-tool-choice=${JSON.stringify(intersection)}]`,'Erase up to intersection','s.commands.find(c=>c.id==="erase_to_intersection").selected');
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
