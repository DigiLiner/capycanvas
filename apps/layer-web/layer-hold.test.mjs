import assert from "node:assert/strict";
import {mkdir,writeFile} from "node:fs/promises";

export async function checkLayerHolding({call, evaluate, settle}) {
  const wait=async(ms=180)=>{await settle();await evaluate(`new Promise(r=>setTimeout(r,${ms}))`);};
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await wait();};
  const capture = async name => {
    if (!process.env.LAYER_TEST_ARTIFACTS) return;
    await mkdir(process.env.LAYER_TEST_ARTIFACTS,{recursive:true});
    const shot = await call("Page.captureScreenshot",{format:"png"});
    await writeFile(`${process.env.LAYER_TEST_ARTIFACTS}/${name}.png`,Buffer.from(shot.data,"base64"));
  };
  const order=()=>evaluate("layerApp.state().layers.map(l=>String(l.id))");
  const menu=()=>evaluate("document.querySelector('.panel-context-menu').matches(':popover-open')");
  const rect=selector=>evaluate(`(()=>{const r=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height};})()`);
  const thumbnailPixels = async selector => {
    await evaluate(`Promise.all(document.querySelector(${JSON.stringify(selector)}).getAnimations().map(animation=>animation.finished.catch(()=>{})))`);
    const shot = await call("Page.captureScreenshot",{format:"png"});
    return evaluate(`(async()=>{
      const button=document.querySelector(${JSON.stringify(selector)}),r=button.getBoundingClientRect(),image=new Image();
      image.src='data:image/png;base64,${shot.data}';await image.decode();
      const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;
      const ctx=canvas.getContext('2d',{willReadFrequently:true});ctx.drawImage(image,0,0);
      const at=(x,y)=>Array.from(ctx.getImageData(Math.floor(x*devicePixelRatio),Math.floor(y*devicePixelRatio),1,1).data);
      return {edge:at(r.x+15,r.y+1),accent:[...getComputedStyle(button).outlineColor.match(/[\\d.]+/g).slice(0,3).map(Number),255],
        corner:at(r.x+2,r.y+2),background:at(r.x-4,r.y+2),inside:at(r.x+4,r.y+4),
        preview:[[15,3],[3,15],[26,15],[15,26]].map(([x,y])=>at(r.x+x,r.y+y))};
    })()`);
  };
  const saved=await evaluate("layerApp.state().workspace");
  let down=false,device="touch",point;
  const input=async(type,p=point)=>{
    point=p;
    if(device==="touch")await call("Input.dispatchTouchEvent",{type:{down:"touchStart",move:"touchMove",up:"touchEnd",cancel:"touchCancel"}[type],touchPoints:["up","cancel"].includes(type)?[]:[{id:1,...p}]});
    else await call("Input.dispatchMouseEvent",{type:{down:"mousePressed",move:"mouseMoved",up:"mouseReleased"}[type],...p,button:"left",buttons:type==="up"?0:1,clickCount:1,pointerType:device});
    down=!["up","cancel"].includes(type);await wait();
  };
  try {
    await send({type:"invoke",command:"reset_layout"});
    for(let i=0;i<2;i++)await send({type:"layer",action:{op:"new",group:false,clipped:false}});
    const before=await order(),source=`#layer-rows .layer-row[data-layer="${before[0]}"]`;
    await send({type:"layer",action:{op:"add_mask",id:Number(before[0]),replace:false}});
    for (const theme of ["light","dark"]) {
      await send({type:"set_theme",theme});
      for (const pointer of ["mouse","pen","touch"]) {
        device=pointer;
        await evaluate("document.querySelector('.panel-context-menu').hidePopover()");
        const r=await rect(`${source} .layer-name`);
        await input("down",{x:r.x+r.width/2,y:r.y+r.height/2});await wait(600);
        assert.equal(await menu(),device!=="mouse",`${theme}/${device}: direct body hold opens menu`);
        const target=await rect(`#layer-rows .layer-row[data-layer="${before[1]}"]`);
        await input("move",{x:target.x+target.width/2,y:target.y+target.height-3});
        assert.equal(await evaluate("document.querySelectorAll('.layer-drag-preview').length"),device==="mouse"?1:0,`${theme}/${device}: body pickup policy`);
        await input("up");
        if(device==="mouse") {
          assert.notDeepEqual(await order(),before);
          await send({type:"invoke",command:"undo"});
        }
        assert.deepEqual(await order(),before);
        await evaluate("document.querySelector('.panel-context-menu').hidePopover()");
        for (const cancel of [false,true]) {
          const grip=await rect(`${source} .layer-grip`);
          await input("down",{x:grip.x+grip.width/2,y:grip.y+grip.height/2});await wait(600);
          assert.equal(await menu(),device!=="mouse",`${theme}/${device}: stationary direct grip hold opens menu`);
          await input("move",{x:target.x+target.width/2,y:target.y+target.height-3});
          assert.equal(await menu(),false,`${theme}/${device}: grip movement dismisses menu`);
          assert.equal(await evaluate("document.querySelectorAll('.layer-drag-preview').length"),1,`${theme}/${device}: grip reorders`);
          if(cancel) {
            await evaluate("window.dispatchEvent(new Event('blur'))");
            await input(device==="touch"?"cancel":"up");
          } else {
            await input("up");assert.notDeepEqual(await order(),before);
            await send({type:"invoke",command:"undo"});assert.deepEqual(await order(),before);
            await send({type:"invoke",command:"redo"});assert.notDeepEqual(await order(),before);
            await send({type:"invoke",command:"undo"});
          }
          assert.deepEqual(await order(),before);
        }
      }
    }
    const click = async (selector, modifiers = 0) => {
      const r = await rect(selector), p = {x:r.x+r.width/2,y:r.y+r.height/2};
      await call("Input.dispatchMouseEvent",{type:"mousePressed",...p,button:"left",buttons:1,clickCount:1,modifiers});
      await call("Input.dispatchMouseEvent",{type:"mouseReleased",...p,button:"left",buttons:0,clickCount:1,modifiers});await wait();
    };
    const checked = () => evaluate("layerApp.state().layers.filter(l=>l.selected).map(l=>String(l.id))");
    for (const theme of ["light","dark"]) {
      await send({type:"set_theme",theme});
      await send({type:"layer",action:{op:"select",id:Number(before[0]),mask:true}});
      await click(`#layer-rows .layer-row[data-layer="${before[1]}"] .layer-icon:nth-child(2)`);
      await click(`${source} .layer-name`);
      assert.equal(await evaluate("layerApp.state().layer_tools.editing_layer.mask_selected"),true,"active row keeps mask editing");
      assert.deepEqual(await checked(),before.slice(0,2),"active row keeps checked companions");
      const mask = `${source} [aria-label="Edit layer mask"]`;
      assert.equal((await rect(mask)).width,30,"thumbnail hit area stays fixed");
      assert.equal((await rect(`${source} .layer-link`)).width,10,"mask link is narrower");
      await call('Emulation.setEmulatedMedia',{features:[{name:'prefers-reduced-motion',value:'no-preference'}]});
      const content = `${source} .layer-thumbnail:not([aria-label="Edit layer mask"])`;
      const selectionAnimation = async (selector, maskSelected) => evaluate(`(async()=>{
        const button=document.querySelector(${JSON.stringify(selector)});
        getComputedStyle(button).outlineWidth;
        layerApp.dispatch({type:'layer',action:{op:'select',id:${Number(before[0])},mask:${maskSelected}}});
        const read=()=>{const style=getComputedStyle(button),color=style.outlineColor.match(/[\\d.]+/g).map(Number);return {width:parseFloat(style.outlineWidth),offset:parseFloat(style.outlineOffset),alpha:color.length===4?color[3]:1};};
        getComputedStyle(button).outlineWidth;
        const animations=button.getAnimations();
        const timing=animations.map(animation=>({property:animation.transitionProperty,...animation.effect.getTiming()}));
        for(const animation of animations){animation.pause();animation.currentTime=100;}
        const middle=read();
        for(const animation of animations)animation.finish();
        await Promise.all(animations.map(animation=>animation.finished));
        return {timing,middle,end:read(),extent:button.getBoundingClientRect().width+2*(read().offset+read().width)};
      })()`);
      for(const [selector,maskSelected] of [[content,false],[mask,true]]) {
        await evaluate("Promise.all([...document.querySelectorAll('.layer-thumbnail')].flatMap(button=>button.getAnimations().map(animation=>animation.finished.catch(()=>{}))))");
        const sample=await selectionAnimation(selector,maskSelected);
        assert.deepEqual(sample.timing.map(t=>t.property).sort(),['outline-color','outline-offset','outline-width'],`${theme}: selection animates all border properties`);
        for(const timing of sample.timing) {
          assert.equal(timing.duration,200);
          assert.equal(timing.easing,'cubic-bezier(0.25, 0.46, 0.45, 0.94)');
        }
        assert.ok(sample.middle.width>0 && sample.middle.width<3,JSON.stringify(sample));
        assert.ok(sample.middle.offset>-2 && sample.middle.offset<4,JSON.stringify(sample));
        assert.ok(sample.middle.alpha>0 && sample.middle.alpha<1,JSON.stringify(sample));
        assert.deepEqual(sample.end,{width:3,offset:-2,alpha:1});
        assert.equal(sample.extent,32);
        console.log(`PASS: ${theme} ${maskSelected?'mask':'content'} selection animation ${JSON.stringify(sample)}`);
      }
      await evaluate(`(()=>{
        const mask=document.querySelector(${JSON.stringify(mask)}),content=document.querySelector(${JSON.stringify(content)});
        layerApp.dispatch({type:'layer',action:{op:'select',id:${Number(before[0])},mask:false}});
        getComputedStyle(content).outlineWidth;
        for(const animation of [...mask.getAnimations(),...content.getAnimations()]){animation.pause();animation.currentTime=70;}
        layerApp.dispatch({type:'layer',action:{op:'select',id:${Number(before[0])},mask:true}});
        getComputedStyle(mask).outlineWidth;
      })()`);
      await evaluate(`Promise.all([...document.querySelector(${JSON.stringify(mask)}).getAnimations(),...document.querySelector(${JSON.stringify(content)}).getAnimations()].map(animation=>animation.finished.catch(()=>{})))`);
      assert.equal(await evaluate(`getComputedStyle(document.querySelector(${JSON.stringify(mask)})).outlineWidth`),'3px','rapid retarget settles on mask');
      assert.equal(await evaluate(`getComputedStyle(document.querySelector(${JSON.stringify(content)})).outlineWidth`),'0px','rapid retarget clears content');
      await call('Emulation.setEmulatedMedia',{features:[{name:'prefers-reduced-motion',value:'reduce'}]});
      try {
        const reduced=await selectionAnimation(content,false);
        assert.equal(reduced.timing.length,0,'reduced motion has no selection animations');
        assert.deepEqual(reduced.end,{width:3,offset:-2,alpha:1});
        await selectionAnimation(mask,true);
      } finally {
        await call('Emulation.setEmulatedMedia',{features:[]});
      }
      const active = await thumbnailPixels(mask);
      assert.deepEqual(active.edge,active.accent,"outer editing edge uses the accent color");
      await click(`${source} .layer-thumbnail`);
      const idle = await thumbnailPixels(mask);
      assert.deepEqual(idle.corner,idle.background,"full squircle clears the preview corner");
      assert.notDeepEqual(idle.inside,idle.background,"full squircle retains pixels beyond a circular corner");
      assert.deepEqual(active.preview,idle.preview,"preview pixels inside the editing border stay unchanged");
      assert.notDeepEqual(active.edge,idle.edge,"editing border replaces the idle edge outside the preview");
      await click(mask);
      await capture(`thumbnail-squircles-${theme}`);
      await click(`${source} .layer-link`);
      assert.equal(await evaluate(`document.querySelector('${source} .layer-link svg').dataset.asset`),"unlink");
      assert.equal(await evaluate(`document.querySelector('${source} .layer-link').getAttribute('aria-pressed')`),"false");
      await capture(`mask-unlinked-${theme}`);
      await click(`${source} .layer-link`);
      assert.equal(await evaluate(`document.querySelector('${source} .layer-link svg').dataset.asset`),"link");
      await capture(`mask-linked-${theme}`);
      await click(`${source} .layer-thumbnail`);
      await click(`#layer-rows .layer-row[data-layer="${before[2]}"] .layer-name`,8);
      assert.deepEqual(await checked(),before.slice(0,3),"Shift selects the visible consecutive range");
      await click(`${source} .layer-thumbnail`);
      await click(`#layer-rows .layer-row[data-layer="${before[2]}"] .layer-icon:nth-child(2)`,8);
      assert.deepEqual(await checked(),before.slice(0,3),"Shift checkbox selects the same range");
      for (const id of before.slice(0,3)) assert.equal(await evaluate(`document.querySelector('#layer-rows .layer-row[data-layer="${id}"] .layer-icon:nth-child(2)').getAttribute('aria-pressed')`),"true");
      await click('.layer-footer button[aria-label="New group"]');
      const group = await evaluate("String(layerApp.state().layer_tools.editing_layer.id)");
      assert.ok((await order()).includes(group));
      assert.equal(await evaluate(`layerApp.state().layers.filter(l=>${JSON.stringify(before.slice(0,3))}.includes(String(l.id))).every(l=>l.depth===1)`),true,"New group contains every checked row");
      await click('.layer-footer button[aria-label="Delete selected layers"]');
      assert.deepEqual(await order(),before,"deleting an expanded folder preserves unchecked children");
      await send({type:"invoke",command:"undo"});await send({type:"invoke",command:"undo"});
      assert.deepEqual(await order(),before);
      await send({type:"layer",action:{op:"select",id:Number(before[0]),mask:false}});
      await send({type:"effect",action:{op:"insert",effect:"curves"}});
      const effect = await evaluate("String(layerApp.state().layer_tools.editing_layer.id)");
      assert.equal(await evaluate(`document.querySelector('#layer-rows .layer-row[data-layer="${effect}"] .layer-thumbnail').classList.contains('editing-target')`),false,"filter icon has no editing border");
      await capture(`filter-selected-${theme}`);
      await send({type:"invoke",command:"undo"});
      await send({type:"layer",action:{op:"select",id:Number(before[0]),mask:false}});
      await click(`#layer-rows .layer-row[data-layer="${before[1]}"] .layer-icon:nth-child(2)`);
      device="mouse";
      const start = await rect(`${source} .layer-name`), target = await rect(`#layer-rows .layer-row[data-layer="${before[2]}"]`);
      await input("down",{x:start.x+start.width/2,y:start.y+start.height/2});
      await input("move",{x:target.x+target.width/2,y:target.y+target.height-3});
      await capture(`thumbnail-drag-${theme}`);await input("up");
      assert.deepEqual(await order(),[before[2],...before.slice(0,2),...before.slice(3)],"drag moves the checked block in stack order");
      await send({type:"invoke",command:"undo"});assert.deepEqual(await order(),before);
      await send({type:"layer",action:{op:"select",id:Number(before[0]),mask:false}});
    }
    device="touch";
    await send({type:"layer",action:{op:"begin_rename",id:Number(before[0])}});
    const entry=await rect(`${source} input`);
    await input("down",{x:entry.x+entry.width/2,y:entry.y+entry.height/2});await wait(650);
    assert.equal(await menu(),false,"editing a name keeps native text input");
    await input("cancel");await send({type:"layer",action:{op:"cancel_rename"}});
    device="touch";
    for(const width of [1440,900]) {
      await call('Emulation.setDeviceMetricsOverride',{width,height:1000,deviceScaleFactor:1,mobile:false});await wait();
      for(const theme of ['light','dark']) {
        await send({type:'set_theme',theme});
        const r=await rect(`${source} .layer-name`);
        await input('down',{x:r.x+r.width/2,y:r.y+r.height/2});await wait(600);
        assert.equal(await menu(),true,`${theme}/${width}: touch hold`);
        const target=await rect(`#layer-rows .layer-row[data-layer="${before[1]}"]`);
        await input('move',{x:target.x+target.width/2,y:target.y+target.height-3});
        assert.equal(await evaluate("document.querySelectorAll('.layer-drag-preview').length"),0,"body hold cannot become reorder");
        await input('cancel');assert.deepEqual(await order(),before);
      }
    }
    await call('Emulation.setDeviceMetricsOverride',{width:1440,height:1000,deviceScaleFactor:1,mobile:false});await wait();
    await evaluate("for(let i=0;i<30;i++)layerApp.dispatch({type:'layer',action:{op:'new',group:false,clipped:false}});document.querySelector('#layer-rows').scrollTop=0;");await wait();
    for (const theme of ["light","dark"]) {
      await send({type:"set_theme",theme});
      device="mouse";
      const body=await rect("#layer-rows .layer-swipe:nth-child(4) .layer-name");
      await input("move",{x:body.x+body.width/2,y:body.y+body.height/2});
      assert.equal(await evaluate("getComputedStyle(document.querySelector('.layer-list > .scroll-thumb')).pointerEvents"),"auto","mouse scrollbar accepts input");
      const thumb=await rect(".layer-list > .scroll-thumb");
      await input("down",{x:thumb.x+thumb.width/2,y:thumb.y+thumb.height/2});
      await input("move",{x:point.x,y:point.y+25});await input("up");
      assert.ok(await evaluate("document.querySelector('#layer-rows').scrollTop")>20,`${theme}: mouse scrollbar scrolls layers`);
      await evaluate("document.querySelector('#layer-rows').scrollTop=0");await wait();
      for (const direct of ["pen","touch"]) {
        device=direct;
        await input("down",{x:thumb.x+thumb.width/2,y:thumb.y+thumb.height/2});
        assert.equal(await evaluate("getComputedStyle(document.querySelector('.layer-list > .scroll-thumb')).pointerEvents"),"none",`${theme}/${direct}: first edge contact disables mouse scrollbar`);
        const gripContact=await evaluate(`!!document.elementFromPoint(${point.x},${point.y})?.closest('.layer-grip')`);
        const edgeOrder=await order();
        await input("move",{x:point.x,y:point.y-85});
        if(gripContact)assert.equal(await evaluate("document.querySelectorAll('.layer-drag-preview').length"),1,`${theme}/${direct}: first edge grip contact reorders`);
        else assert.ok(await evaluate("document.querySelector('#layer-rows').scrollTop")>20,`${theme}/${direct}: first edge body contact pans layers`);
        assert.equal(await evaluate("[1,2].some(id=>document.querySelector('.layer-list > .scroll-thumb').hasPointerCapture(id))"),false,"direct contact never captures scrollbar");
        if(gripContact){await evaluate("window.dispatchEvent(new Event('blur'))");await input(device==="touch"?"cancel":"up");}
        else await input("up");
        assert.deepEqual(await order(),edgeOrder,'first direct edge contact preserves order after pan or cancelled pickup');
        await evaluate("document.querySelector('#layer-rows').scrollTop=0");await wait();
        device="mouse";await input("move",{x:body.x+body.width/2,y:body.y+body.height/2});
      }
    }
    device="touch";
    const scrollingOrder=await order(),r=await rect("#layer-rows .layer-swipe:nth-child(4) .layer-name");
    await input("down",{x:r.x+r.width/2,y:r.y+r.height/2});
    await input("move",{x:point.x,y:point.y-85});await input("up");await wait(650);
    assert.ok(await evaluate("document.querySelector('#layer-rows').scrollTop")>20,"movement before hold scrolls normally");
    assert.equal(await menu(),false);assert.deepEqual(await order(),scrollingOrder);
    for (const deviceType of ["mouse","touch","pen"]) {
      device=deviceType;
      await evaluate("document.querySelector('#layer-rows').scrollTop=0");await wait();
      const source = await rect("#layer-rows .layer-swipe:nth-child(2) .layer-grip"), viewport = await rect("#layer-rows");
      await input("down",{x:source.x+source.width/2,y:source.y+source.height/2});
      if (device!=="mouse") await wait(600);
      await input("move",{x:viewport.x+viewport.width/2,y:viewport.y+viewport.height-8});
      await wait(450);
      assert.ok(await evaluate("document.querySelector('#layer-rows').scrollTop")>40,`${device}: dragging at the edge scrolls to offscreen destinations`);
      await evaluate("window.dispatchEvent(new Event('blur'))");await input(device==="touch"?"cancel":"up");
      const stopped = await evaluate("document.querySelector('#layer-rows').scrollTop");await wait(250);
      assert.equal(await evaluate("document.querySelector('#layer-rows').scrollTop"),stopped,"cancelled drag stops edge scrolling");
      assert.deepEqual(await order(),scrollingOrder);
    }
    const lockedPickup = async (id, owner, label) => {
      const row = `#layer-rows .layer-row[data-layer="${id}"]`;
      const locked = () => evaluate(`layerApp.state().layers.find(l=>Number(l.id)===${id}).locked`);
      for (const theme of ["light","dark"]) {
        await send({type:"set_theme",theme});
        await send({type:"layer",action:{op:"lock",id:owner,value:true}});
        assert.equal(await locked(),true,`${label}/${theme}: effective lock`);
        assert.equal(await evaluate(`layerApp.state().layers.find(l=>Number(l.id)===${id}).can_drop_below`),true,"locked row remains a drop surface");
        const before = await order();
        const targetId = Number(before.find(value=>Number(value)!==id && Number(value)!==owner));
        for (const pointer of ["mouse","pen","touch"]) {
          device=pointer;
          for (const part of ["layer-name","layer-grip"]) {
            await evaluate("document.querySelector('#layer-rows').scrollTop=0");await wait();
            const from=await rect(`${row} .${part}`), to=await rect(`#layer-rows .layer-row[data-layer="${targetId}"]`);
            await input("down",{x:from.x+from.width/2,y:from.y+from.height/2});
            await input("move",{x:from.x+from.width/2,y:to.y+to.height-3});
            await capture(`locked-${label}-${theme}-${device}-${part}`);
            assert.equal(await evaluate("document.querySelectorAll('.layer-drag-preview').length"),0,`${label}/${theme}/${device}/${part}: locked source cannot pick up`);
            assert.equal(await evaluate("document.querySelectorAll('.layer-drop-before,.layer-drop-after,.layer-drop-into,.layer-drop-attach').length"),0,"locked source cannot preview a drop");
            await input("up");assert.deepEqual(await order(),before,"locked contact preserves order");
            assert.equal(await evaluate(`parseFloat(document.querySelector('${row}').parentElement.style.getPropertyValue('--swipe'))`),0,"vertical locked contact leaves the row at rest");
          }
        }
        assert.equal(await evaluate(`document.querySelector('${row} .layer-grip').ariaDisabled`),"true","locked grip remains visibly disabled");
        await send({type:"invoke",command:"undo"});assert.equal(await locked(),false,"Undo unlocks the effective source");
        await send({type:"invoke",command:"redo"});assert.equal(await locked(),true,"Redo restores the effective lock");
        await send({type:"layer",action:{op:"lock",id:owner,value:false}});
        assert.equal(await locked(),false);assert.equal(await evaluate(`document.querySelector('${row} .layer-grip').ariaDisabled`),"false");
        for (const pointer of ["mouse","pen","touch"]) {
          device=pointer;
          await evaluate("document.querySelector('#layer-rows').scrollTop=0");await wait();
          const from=await rect(`${row} .layer-grip`), to=await rect(`#layer-rows .layer-row[data-layer="${targetId}"]`);
          await input("down",{x:from.x+from.width/2,y:from.y+from.height/2});
          await input("move",{x:to.x+to.width/2,y:to.y+to.height-3});
          assert.equal(await evaluate("document.querySelectorAll('.layer-drag-preview').length"),1,`${label}/${theme}/${device}: unlocking restores grip pickup`);
          await input("up");assert.notDeepEqual(await order(),before);
          await send({type:"invoke",command:"undo"});assert.deepEqual(await order(),before);
          await send({type:"invoke",command:"redo"});assert.notDeepEqual(await order(),before);
          await send({type:"invoke",command:"undo"});assert.deepEqual(await order(),before);
        }
      }
    };
    const own = Number((await order())[0]);await lockedPickup(own,own,"own");
    await send({type:"layer",action:{op:"new",group:true,clipped:false}});
    const owner=await evaluate("Number(layerApp.state().layer_tools.editing_layer.id)");
    await send({type:"layer",action:{op:"new",group:false,clipped:false}});
    const child=await evaluate("Number(layerApp.state().layer_tools.editing_layer.id)");
    await lockedPickup(child,owner,"parent");
  console.log("PASS: layer range/group/delete, checked drag, thumbnail squircles and outer editing borders, mask link, accessibility, edge scrolling, direct body menus, grip reorder and undo/redo");
  } finally {
    if(down)await input(device==="touch"?"cancel":"up");
    await evaluate("document.querySelector('.panel-context-menu').hidePopover()");
    await send({type:"restore_workspace",workspace:saved});
  }
}

export async function checkLayerSwipes({call, evaluate, settle}) {
  const wait=async()=>{await settle();await evaluate("new Promise(r=>setTimeout(r,200))");};
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await wait();};
  let targetId=1;
  const layer=()=>evaluate(`(()=>{const l=layerApp.state().layers.find(l=>Number(l.id)===${targetId});return{alpha_locked:l.alpha_locked,pass_through:l.pass_through,blend:l.blend_label}})()`);
  const rect=()=>evaluate(`(()=>{const r=document.querySelector('#layer-rows .layer-row[data-layer="${targetId}"]').parentElement.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};})()`);
  const revealed=()=>evaluate(`parseFloat(document.querySelector('#layer-rows .layer-row[data-layer="${targetId}"]').parentElement.style.getPropertyValue('--swipe'))>0`);
  const swipe=async(device,dx,{cancel=false,returnToStart=false}={})=>{
    const start=await rect();
    const input=async(type,x)=>{
      if(device==='touch')await call('Input.dispatchTouchEvent',{type,touchPoints:['touchEnd','touchCancel'].includes(type)?[]:[{id:1,x,y:start.y}]});
      else await call('Input.dispatchMouseEvent',{type:{touchStart:'mousePressed',touchMove:'mouseMoved',touchEnd:'mouseReleased'}[type],x,y:start.y,button:'left',buttons:type==='touchEnd'?0:1,clickCount:1,pointerType:device});
    };
    await input('touchStart',start.x);
    await input('touchMove',start.x+dx/3);await input('touchMove',start.x+dx);
    if(returnToStart)await input('touchMove',start.x);
    if(cancel && device!=='touch')await evaluate("window.dispatchEvent(new Event('blur'))");
    await input(cancel && device==='touch'?'touchCancel':'touchEnd',start.x+(returnToStart?0:dx));await wait();
  };
  await send({type:'invoke',command:'reset_layout'});
  await send({type:'layer',action:{op:'alpha_lock',id:1,value:false}});
  for(const theme of ['light','dark']) {
    await send({type:'set_theme',theme});
    for(const device of ['touch','pen','mouse']) {
      for(const options of [{dx:18},{dx:60,cancel:true},{dx:60,returnToStart:true}]) {
        await swipe(device,options.dx,options);assert.equal((await layer()).alpha_locked,false,`${device}: unfinished swipe`);
      }
      await swipe(device,60);assert.equal((await layer()).alpha_locked,device!=='mouse',`${theme}/${device}: swipe right`);
      if(device==='mouse')continue;
      await send({type:'invoke',command:'undo'});assert.equal((await layer()).alpha_locked,false);
      await send({type:'invoke',command:'redo'});assert.equal((await layer()).alpha_locked,true);
      await swipe(device,60);assert.equal((await layer()).alpha_locked,false,'second swipe unlocks');
      await swipe(device,-60);assert.equal(await revealed(),true,'left swipe reveals Delete');
      await swipe(device,90);assert.equal(await revealed(),false,'reverse swipe closes Delete');
      assert.equal((await layer()).alpha_locked,false,'closing Delete does not toggle alpha lock');
      await send({type:'layer',action:{op:'lock',id:1,value:true}});
      await swipe(device,60);assert.equal((await layer()).alpha_locked,false,'locked layer refuses alpha lock');
      await send({type:'layer',action:{op:'lock',id:1,value:false}});
    }
  }
  await send({type:'layer',action:{op:'new',group:true,clipped:false}});
  targetId=await evaluate('Number(layerApp.state().layer_tools.editing_layer.id)');
  await send({type:'layer',action:{op:'blend',id:targetId,value:1}});
  for(const theme of ['light','dark']) {
    await send({type:'set_theme',theme});
    for(const device of ['touch','pen']) {
      await send({type:'layer',action:{op:'blend',id:targetId,value:1}});
      for(const options of [{dx:18},{dx:60,cancel:true},{dx:60,returnToStart:true}]) {
        await swipe(device,options.dx,options);assert.equal((await layer()).blend,'Multiply',`${device}: unfinished group swipe`);
      }
      await swipe(device,60);assert.equal((await layer()).pass_through,true);
      assert.equal(await evaluate(`!!document.querySelector('#layer-rows .layer-row[data-layer="${targetId}"] .layer-group-pass-through')`),true);
      await send({type:'invoke',command:'undo'});assert.equal((await layer()).blend,'Multiply');
      await send({type:'invoke',command:'redo'});assert.equal((await layer()).pass_through,true);
      await swipe(device,60);assert.equal((await layer()).blend,'Normal','disabling Pass Through selects Normal');
      await swipe(device,-60);assert.equal(await revealed(),true);
      await swipe(device,90);assert.equal(await revealed(),false);
      assert.equal((await layer()).blend,'Normal','closing group Delete does not change mode');
      await send({type:'layer',action:{op:'lock',id:targetId,value:true}});
      await swipe(device,60);assert.equal((await layer()).blend,'Normal','locked group has no right action');
      await send({type:'layer',action:{op:'lock',id:targetId,value:false}});
    }
  }
  await send({type:'layer',action:{op:'select',id:targetId,mask:false}});
  await send({type:'effect',action:{op:'insert',effect:'curves'}});
  const effect=await evaluate('Number(layerApp.state().layer_tools.editing_layer.id)');
  await send({type:'layer',action:{op:'clip',id:effect,value:true}});
  assert.equal(await evaluate(`layerApp.state().layers.find(l=>Number(l.id)===${targetId}).right_swipe??null`),null);
  await swipe('touch',60);assert.equal((await layer()).blend,'Normal','attached FX protect group isolation');
  for(const theme of ['light','dark']) {
    await send({type:'set_theme',theme});
    await swipe('touch',-60);
    assert.equal(await revealed(),true,'displaced row exposes Delete');
    assert.equal(await evaluate(`document.querySelector('#layer-rows .layer-row[data-layer="${targetId}"] .layer-grip').hidden`),true,'displaced row hides grip');
    const p=await rect();
    await call('Input.dispatchMouseEvent',{type:'mousePressed',...p,button:'left',buttons:1,clickCount:1});
    await call('Input.dispatchMouseEvent',{type:'mouseMoved',x:p.x,y:p.y+65,button:'left',buttons:1});
    assert.equal(await evaluate("document.querySelectorAll('.layer-drag-preview').length"),0,'contact starting on displaced row cannot pick up');
    await call('Input.dispatchMouseEvent',{type:'mouseReleased',x:p.x,y:p.y+65,button:'left',buttons:0,clickCount:1});
    await wait();
    if(await revealed())await swipe('touch',90);
  }
  const column=await evaluate("layerApp.app.layout(innerWidth,innerHeight).groups.find(g=>g.panels.includes('layers')).id");
  for(const theme of ['light','dark']) {
    await send({type:'set_theme',theme});
    await swipe('touch',-60);
    assert.equal(await revealed(),true,'swipe is open before panel lifecycle change');
    const outside=await evaluate("(()=>{const r=document.querySelector('#document-title').getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};})()");
    await call('Input.dispatchMouseEvent',{type:'mousePressed',...outside,button:'left',buttons:1,clickCount:1});
    const closing=await evaluate(`(()=>{const row=document.querySelector('#layer-rows .layer-row[data-layer="${targetId}"]');const result={transform:getComputedStyle(row).transform,animations:row.getAnimations().map(a=>a.transitionProperty)};layerApp.dispatch({type:'customize',action:{type:'set_column_collapsed',group:${column},collapsed:true}});return result;})()`);
    assert.ok(closing.animations.includes('transform'),'panel collapses while swipe return transition is active');
    await call('Input.dispatchMouseEvent',{type:'mouseReleased',...outside,button:'left',buttons:0,clickCount:1});
    await wait();
    await send({type:'customize',action:{type:'set_column_collapsed',group:column,collapsed:false}});
    const reopened=await evaluate(`(()=>{const row=document.querySelector('#layer-rows .layer-row[data-layer="${targetId}"]');return{transform:new DOMMatrixReadOnly(getComputedStyle(row).transform).m41,gripHidden:row.querySelector('.layer-grip').hidden,width:row.getBoundingClientRect().width};})()`);
    console.log('Swipe panel lifecycle',theme,{closing,reopened});
    assert.ok(reopened.width>0,'reopened swipe row is visible');
    assert.equal(reopened.transform,0,'reopened swipe row is visually at rest');
    assert.equal(reopened.gripHidden,false,`${theme}: cancelled closing transition restores grip on reopen`);
    for(const pointer of ['mouse','touch']) {
      const points=await evaluate(`(()=>{const row=document.querySelector('#layer-rows .layer-row[data-layer="${targetId}"]'),source=row.querySelector('${pointer==='mouse'?'.layer-name':'.layer-grip'}').getBoundingClientRect(),target=document.querySelector('#layer-rows .layer-row[data-layer="1"]').getBoundingClientRect();return{source:{x:source.x+source.width/2,y:source.y+source.height/2},target:{x:target.x+target.width/2,y:target.bottom-2}};})()`);
      if(pointer==='touch') {
        await call('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{id:1,...points.source}]});
        await call('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{id:1,...points.target}]});
      } else {
        await call('Input.dispatchMouseEvent',{type:'mousePressed',...points.source,button:'left',buttons:1,clickCount:1});
        await call('Input.dispatchMouseEvent',{type:'mouseMoved',...points.target,button:'left',buttons:1});
      }
      assert.equal(await evaluate("document.querySelectorAll('.layer-drag-preview').length"),1,`${theme}/${pointer}: reopened resting row can pick up`);
      await evaluate("window.dispatchEvent(new Event('blur'))");
      if(pointer==='touch')await call('Input.dispatchTouchEvent',{type:'touchCancel',touchPoints:[]});
      else await call('Input.dispatchMouseEvent',{type:'mouseReleased',...points.target,button:'left',buttons:0,clickCount:1});
      await wait();
    }
  }
  console.log('PASS: paint/group shared swipe actions, themes, touch/pen/mouse, Pass Through to Normal, undo/redo, cancelled swipes, Delete closure, lock and attached-content protection');
}
