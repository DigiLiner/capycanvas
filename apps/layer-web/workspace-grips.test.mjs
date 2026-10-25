import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';

export async function checkWorkspaceGrips({call,evaluate,settle}) {
  const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  const view=()=>evaluate('JSON.parse(layerApp.app.workspace_view())');
  const layout=()=>evaluate('layerApp.app.workspace_persistence().layout');
  const idle=async()=>{for(let n=0;n<300;n++){const v=await view();if(v.ready&&!v.busy&&!v.switcher_busy&&!v.dirty){await settle();return;}await pause(20);}throw Error('Workspace grip request did not settle');};
  const send=async input=>{await evaluate(`layerApp.app.workspace_input(${JSON.stringify(JSON.stringify(input))});null`);await idle();};
  const row=id=>`.workspace-list > .workspace-row[data-id=${JSON.stringify(id)}]`;
  const point=async(selector,y=.5)=>evaluate(`(()=>{const n=document.querySelector(${JSON.stringify(selector)});if(!n)throw Error('Missing '+${JSON.stringify(selector)});const r=n.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height*${y}}})()`);
  let device='mouse';
  const pointer=async(type,p)=>{if(device==='touch')await call('Input.dispatchTouchEvent',{type:{down:'touchStart',move:'touchMove',up:'touchEnd',cancel:'touchCancel'}[type],touchPoints:['up','cancel'].includes(type)?[]:[{id:1,...p}]});else await call('Input.dispatchMouseEvent',{type:{down:'mousePressed',move:'mouseMoved',up:'mouseReleased'}[type],...p,pointerType:device,button:'left',buttons:type==='up'?0:1,clickCount:1});await pause(30);};
  const tap=async selector=>{const p=await point(selector);await pointer('down',p);await pointer('up',p);await idle();};
  const click=async selector=>{device='mouse';await tap(selector);};
  const key=async name=>{const code={Enter:13,' ':32,Escape:27,F2:113,Home:36}[name];for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:name,code:name===' '?'Space':name,windowsVirtualKeyCode:code,...(type==='keyDown'&&!['Escape','F2','Home'].includes(name)?{text:name==='Enter'?'\r':' ',unmodifiedText:name==='Enter'?'\r':' '}:{})});await idle();};
  const menuOpen=()=>evaluate('document.querySelector(".workspace-row-menu").matches(":popover-open")');
  const pins=async()=>new Set((await view()).switcher.map(row=>row.id));
  const p='builtin:workspace:painter',f='builtin:workspace:photographer';
  const output=process.env.LAYER_TEST_ARTIFACTS||'artifacts/workspace-grips';await mkdir(output,{recursive:true});
  await idle();const original=await view();
  await evaluate(`window.workspaceGripEvents=[];const record=(e,phase)=>{workspaceGripEvents.push({type:e.type,phase,key:e.key,target:e.target.closest?.("button")?.className,same:e.target===window.retainedKeyboardGrip,prevented:e.defaultPrevented});if(workspaceGripEvents.length>30)workspaceGripEvents.shift()};for(const type of ["pointerdown","pointerup","click"])document.addEventListener(type,e=>record(e,"capture"),true);for(const type of ["keydown","keypress","keyup"])window.addEventListener(type,e=>record(e,"window-bubble"));`);
  for(const theme of ['dark','light']) {
    await evaluate(`layerApp.dispatch({type:'set_theme',theme:${JSON.stringify(theme)}});null`);await idle();
    await send({type:'open',page:'workspaces'});
    const creation=await evaluate('(()=>{const add=document.querySelector(".workspace-row-actions .workspace-add"),a=add.getBoundingClientRect(),trash=document.querySelector(".workspace-delete").getBoundingClientRect(),l=document.querySelector(".workspace-list").getBoundingClientRect();return {label:add.textContent.trim(),size:[a.width,a.height],trash:[trash.width,trash.height],left:a.left,listLeft:l.left,gap:a.top-l.bottom,trashGap:trash.left-a.right}})()');
    assert.equal(creation.label,'');assert.deepEqual(creation.size,[34,34]);assert.deepEqual(creation.trash,[34,34]);assert.equal(creation.left,creation.listLeft);assert.equal(creation.gap,6);assert.equal(creation.trashGap,4);
    assert.equal(await evaluate('[...document.querySelectorAll(".workspace-intro,.workspace-search")].every(n=>n.hidden||!n.getClientRects().length)'),true);
    await click('.workspace-row-actions .workspace-add');await evaluate('document.querySelector(".workspace-form input").focus();document.querySelector(".workspace-form input").select()');await call('Input.insertText',{text:`Grip study ${theme}`});await click('.workspace-form .suggested-action');const custom=(await view()).id;assert.equal((await view()).page,'workspaces');assert.equal((await view()).selected,custom);
    let normalFont;
    for(const enlarged of [false,true]) {
      const font=enlarged?'large':'normal';
      await evaluate(`(()=>{window.workspaceParityText?.remove();if(${enlarged}){window.workspaceParityText=document.createElement('style');window.workspaceParityText.textContent=':root { --ui-text-size: 24px !important; }';document.head.append(window.workspaceParityText)}})()`);await settle();
      const geometry=()=>evaluate(`(()=>{const row=document.querySelector(${JSON.stringify(row(custom))}),input=row.querySelector('.workspace-name-entry'),title=row.querySelector('.workspace-row-title'),sub=row.querySelector('.workspace-row-subtitle'),rect=e=>{const r=e.getBoundingClientRect();return[r.x,r.y,r.width,r.height]},s=getComputedStyle(input??title),canvas=document.createElement('canvas'),ctx=canvas.getContext('2d');ctx.font=s.font;const m=ctx.measureText('Mg'),ascent=m.fontBoundingBoxAscent,descent=m.fontBoundingBoxDescent;let text;if(input){const r=input.getBoundingClientRect(),top=parseFloat(s.borderTopWidth)+parseFloat(s.paddingTop),bottom=parseFloat(s.borderBottomWidth)+parseFloat(s.paddingBottom);text=[r.x+parseFloat(s.borderLeftWidth)+parseFloat(s.paddingLeft),r.y+top+(r.height-top-bottom-ascent-descent)/2+ascent]}else{const range=document.createRange();range.selectNodeContents(title);const r=range.getBoundingClientRect();text=[r.x,r.y+ascent]}const reference=document.querySelector('.panel .number-entry'),ref=getComputedStyle(reference);return{row:rect(row),subtitle:rect(sub),text,font:s.font,fontSize:parseFloat(s.fontSize),field:input?{bounds:rect(input),heightMetric:parseFloat(s.getPropertyValue('--panel-control-height')),padding:[s.paddingLeft,s.paddingRight],background:s.backgroundColor,radius:s.borderRadius,reference:{padding:[ref.paddingLeft,ref.paddingRight],background:ref.backgroundColor,radius:ref.borderRadius}}:null}})()`);
      const before=await geometry();if(enlarged)assert.ok(before.fontSize>normalFont,'enlarged text uses larger rendered font');else normalFont=before.fontSize;
      const shot=async phase=>{const capture=await call('Page.captureScreenshot',{format:'png'});await writeFile(`${output}/approved-parity-${theme}-${font}-${phase}.png`,Buffer.from(capture.data,'base64'));};
      await shot('before');
      const titlePoint=await point(`${row(custom)} .workspace-row-title`);
      for(const count of [1,2])for(const type of ['mousePressed','mouseReleased'])await call('Input.dispatchMouseEvent',{type,...titlePoint,button:'left',buttons:type==='mousePressed'?1:0,clickCount:count});await idle();
      const during=await geometry();await shot('selected');await key('Home');await shot('cursor');
      await writeFile(`${output}/approved-parity-${theme}-${font}-measurement.json`,JSON.stringify({before,during},null,2));
      assert.deepEqual(during.row,before.row,'inline Rename preserves row bounds');assert.deepEqual(during.subtitle,before.subtitle,'inline Rename preserves subtitle bounds');
      assert.ok(during.text.every((value,index)=>Math.abs(value-before.text[index])<=1),`inline Rename preserves text origin/baseline: ${JSON.stringify({before,during})}`);
      assert.ok(during.field.bounds[3]>=during.field.heightMetric,'editor fits shared compact height');assert.deepEqual(during.field.padding,during.field.reference.padding);assert.equal(during.field.background,during.field.reference.background);assert.equal(during.field.radius,during.field.reference.radius);
      await key('Escape');const after=await geometry();assert.deepEqual(after.row,before.row);assert.deepEqual(after.subtitle,before.subtitle);assert.deepEqual(after.text,before.text);await shot('after');
      const commitPoint=await point(`${row(custom)} .workspace-row-title`);for(const count of [1,2])for(const type of ['mousePressed','mouseReleased'])await call('Input.dispatchMouseEvent',{type,...commitPoint,button:'left',buttons:type==='mousePressed'?1:0,clickCount:count});await idle();
      const accepted=`Parity ${theme} ${font}`;await call('Input.insertText',{text:accepted});await key('Enter');assert.equal((await view()).rows.find(item=>item.id===custom).title,accepted);await shot('committed');
      const actionGeometry=await evaluate(`(()=>{const list=document.querySelector('.workspace-list').getBoundingClientRect();return[...document.querySelectorAll('.workspace-row-actions button')].map(button=>{const r=button.getBoundingClientRect();return{class:button.className,gap:r.top-list.bottom,height:r.height}})})()`);assert.ok(actionGeometry.every(action=>action.gap===6&&action.height===34));
      for(const action of ['workspace-add','workspace-delete']) {const selector=`.workspace-row-actions .${action}`,base=await evaluate(`getComputedStyle(document.querySelector(${JSON.stringify(selector)})).backgroundColor`),at=await point(selector);await call('Input.dispatchMouseEvent',{type:'mouseMoved',...at});await settle();const hover=await evaluate(`getComputedStyle(document.querySelector(${JSON.stringify(selector)})).backgroundColor`);assert.notEqual(hover,base,'footer action has a visible hover background');await shot(`${action}-hover`);}
      await writeFile(`${output}/approved-parity-${theme}-${font}.json`,JSON.stringify({before,during,after,actions:actionGeometry},null,2));
      await evaluate('window.workspaceParityText?.remove()');await settle();
    }
    await send({type:'switch',id:original.id});await send({type:'open',page:'workspaces'});await click(`${row(f)} .workspace-choice`);
    const preview=await layout(),savedPins=await pins(),savedOrder=(await view()).order;
    assert.equal(await evaluate('document.querySelectorAll(".workspace-options,.workspace-options-slot").length'),0,'no row exposes options dots or their empty slot');
    assert.equal(await evaluate('[...document.querySelectorAll(".workspace-row")].every(row=>{const pin=row.querySelector(".workspace-pin").getBoundingClientRect(),grip=row.querySelector(".workspace-grip").getBoundingClientRect();return grip.left>=pin.right})'),true,'grips follow pins on the right');
    const unchanged=async()=>{assert.equal((await view()).id,original.id);assert.equal((await view()).selected,f);assert.deepEqual(await layout(),preview);assert.deepEqual(await pins(),savedPins);};
    for(const kind of ['mouse','touch','pen']) {
      device=kind;await tap(`${row(p)} .workspace-grip`);assert.equal(await menuOpen(),true,`${kind}: a grip tap opens its menu`);
      assert.deepEqual(await evaluate('[...document.querySelectorAll(".workspace-row-menu button")].map(n=>n.dataset.action)'),['up','down']);await unchanged();
      await click('.workspace-row-menu [data-action="down"]');await unchanged();assert.deepEqual((await view()).order,[savedOrder[1],savedOrder[0],...savedOrder.slice(2)]);
      device=kind;await tap(`${row(p)} .workspace-grip`);await click('.workspace-row-menu [data-action="up"]');assert.deepEqual((await view()).order,savedOrder);await unchanged();
      device=kind;const start=await point(`${row(p)} .workspace-grip`),end=await point(row(f),.85);await pointer('down',start);
      for(const t of [.3,.7,1])await pointer('move',{x:start.x+(end.x-start.x)*t,y:start.y+(end.y-start.y)*t});assert.equal(await menuOpen(),false,`${kind}: immediate grip drag does not open a menu`);
      await pointer('up',end);await idle();assert.deepEqual((await view()).order,[savedOrder[1],savedOrder[2],savedOrder[0],...savedOrder.slice(3)]);await unchanged();
      device=kind;const from=await point(`${row(p)} .workspace-grip`),to=await point(row(savedOrder[1]),.15);await pointer('down',from);for(const t of [.3,.7,1])await pointer('move',{x:from.x+(to.x-from.x)*t,y:from.y+(to.y-from.y)*t});await pointer('up',to);await idle();assert.deepEqual((await view()).order,savedOrder);await unchanged();
    }
    device='touch';const cancelledStart=await point(`${row(p)} .workspace-grip`),cancelledEnd=await point(row(f),.85);await pointer('down',cancelledStart);await pointer('move',cancelledEnd);await pointer('cancel',cancelledEnd);await idle();assert.equal(await menuOpen(),false);assert.deepEqual((await view()).order,savedOrder);await unchanged();
    for(const activation of ['Enter',' ']) {
      const beforeKey=await evaluate(`(()=>{const grip=document.querySelector(${JSON.stringify(`${row(p)} .workspace-grip`)});window.retainedKeyboardGrip=grip;grip.focus();return {disabled:grip.disabled,focused:document.activeElement===grip,connected:grip.isConnected,rowClasses:grip.closest('.workspace-row').className,menu:document.querySelector('.workspace-row-menu').matches(':popover-open')}})()`);
      await key(activation);const opened=await menuOpen();
      const afterKey=await evaluate(`({disabled:retainedKeyboardGrip.disabled,connected:retainedKeyboardGrip.isConnected,same:retainedKeyboardGrip===document.querySelector(${JSON.stringify(`${row(p)} .workspace-grip`)}),focused:document.activeElement===retainedKeyboardGrip,rowClasses:retainedKeyboardGrip.closest('.workspace-row')?.className})`);
      assert.equal(opened,true,`keyboard activation opens the grip menu: ${JSON.stringify({theme,activation,beforeKey,afterKey,events:await evaluate('workspaceGripEvents.slice(-12)')})}`);await unchanged();await key('Escape');
      assert.equal(await evaluate(`document.activeElement===document.querySelector(${JSON.stringify(`${row(p)} .workspace-grip`)})`),true,'Escape returns focus to the grip');
    }
    const name=`Renamed grip study ${theme}`;
    const edit=async text=>{await evaluate('(()=>{const n=document.querySelector(".workspace-name-entry");if(!n)throw Error("Missing inline name editor");n.focus();n.select()})()');await call('Input.insertText',{text});};
    const titlePoint=await point(`${row(custom)} .workspace-row-title`);
    device='mouse';for(const count of [1,2]){await call('Input.dispatchMouseEvent',{type:'mousePressed',...titlePoint,button:'left',buttons:1,clickCount:count});await call('Input.dispatchMouseEvent',{type:'mouseReleased',...titlePoint,button:'left',buttons:0,clickCount:count});}await idle();
    assert.equal(await evaluate('document.querySelector(".workspace-form").open'),false,'Rename stays inline');await edit(name);await key('Enter');assert.equal((await view()).rows.find(item=>item.id===custom).title,name);
    await click(`${row(f)} .workspace-choice`);await unchanged();
    await evaluate(`document.querySelector(${JSON.stringify(`${row(custom)} .workspace-choice`)}).focus()`);await key('F2');await edit('Cancelled draft');await key('Escape');assert.equal((await view()).rows.find(item=>item.id===custom).title,name);assert.equal((await view()).page,'workspaces');await unchanged();
    const contextPoint=await point(`${row(custom)} .workspace-choice`);await call('Input.dispatchMouseEvent',{type:'mousePressed',...contextPoint,button:'right',buttons:2,clickCount:1});await call('Input.dispatchMouseEvent',{type:'mouseReleased',...contextPoint,button:'right',buttons:0,clickCount:1});await idle();
    assert.deepEqual(await evaluate('[...document.querySelectorAll(".workspace-row-menu button")].map(n=>n.dataset.action)'),['rename','delete','up','down'],'row context contains metadata then ordering, grip contains ordering only');await click('.workspace-row-menu [data-action="rename"]');await edit(name);await key('Enter');await unchanged();
    device='touch';const hold=await point(`${row(custom)} .workspace-choice`);await pointer('down',hold);await pause(600);await pointer('up',hold);await idle();assert.equal(await menuOpen(),true,'touch hold opens metadata menu');await key('Escape');await unchanged();
    const shot=await call('Page.captureScreenshot',{format:'png'});await writeFile(`${output}/${theme}.png`,Buffer.from(shot.data,'base64'));
    const swipeLeft=async()=>{device='touch';const swipe=await point(`${row(custom)} .workspace-choice`),end={x:swipe.x-100,y:swipe.y};await pointer('down',swipe);for(const t of [.3,.7,1])await pointer('move',{x:swipe.x-100*t,y:swipe.y});await pointer('up',end);await idle();await unchanged();};
    await swipeLeft();await click('.workspace-manager .dialog-header h2');assert.equal(await evaluate(`document.querySelector(${JSON.stringify(`${row(custom)} .workspace-swipe-delete`)}).hidden`),true,'an outside press closes the revealed Delete action');await unchanged();await swipeLeft();
    await click(`${row(custom)} .workspace-swipe-delete`);assert.equal((await view()).prompt_action.type,'delete');await unchanged();await click('.workspace-form .suggested-action');assert.equal((await view()).rows.some(item=>item.id===custom),false);assert.equal((await view()).selected,f);assert.equal((await view()).page,'workspaces');assert.deepEqual(await layout(),preview);
    await click('.workspace-manager footer button');await idle();
  }
  console.log('PASS: bottom list actions, right grips, inline Rename, metadata context/hold, swipe Delete, and retained previews in both themes');
}

export async function checkWorkspaceHeldName({call,evaluate,settle}) {
  const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  const view=()=>evaluate('JSON.parse(layerApp.app.workspace_view())');
  const layout=()=>evaluate('layerApp.app.workspace_persistence().layout');
  const wait=async(predicate,label)=>{for(let n=0;n<300;n++){if(await predicate()){await settle();return;}await pause(20);}throw Error(`Timed out: ${label}; ${JSON.stringify(await view())}`);};
  const idle=()=>wait(async()=>{const v=await view();return v.ready&&!v.busy&&!v.switcher_busy&&!v.dirty;},'workspace idle');
  const send=async input=>{await evaluate(`layerApp.app.workspace_input(${JSON.stringify(JSON.stringify(input))});null`);await idle();};
  const point=selector=>evaluate(`(()=>{const n=document.querySelector(${JSON.stringify(selector)}),r=n?.getBoundingClientRect();if(!r?.width||!r.height)throw Error('Missing '+${JSON.stringify(selector)});return{x:r.x+r.width/2,y:r.y+r.height/2}})()`);
  const pointer=async(type,p)=>{await call('Input.dispatchMouseEvent',{type,...p,button:'left',buttons:type==='mouseReleased'?0:1,clickCount:1});await pause(100);await settle();};
  await idle();const original=(await view()).id,photo='builtin:workspace:photographer',next='builtin:workspace:painter';
  const storageGate=evaluate;
  await evaluate(`(()=>{const post=Worker.prototype.postMessage,workers=new WeakMap();window.heldName={hold:false,responses:[],release(){this.hold=false;for(const deliver of this.responses.splice(0))deliver()},restore(){this.release();Worker.prototype.postMessage=post}};Worker.prototype.postMessage=function(message,...args){if(heldName.hold&&message.request&&JSON.parse(message.request).type==='commit'){let ids=workers.get(this);if(!ids){ids=new Set();workers.set(this,ids);const receive=this.onmessage;this.onmessage=event=>{if(heldName.hold&&ids.has(event.data.id))heldName.responses.push(()=>receive.call(this,event));else receive.call(this,event)}}ids.add(message.id)}return post.call(this,message,...args)};return true})()`);
  await evaluate(`(()=>{window.heldNameTrace={inputs:[],events:[]};heldName.input=layerApp.app.workspace_input;layerApp.app.workspace_input=function(value){heldNameTrace.inputs.push(JSON.parse(value));return heldName.input.call(this,value)};for(const type of ['pointerdown','pointerup','click'])window.addEventListener(type,event=>{heldNameTrace.events.push({type,target:event.target.className,prevented:event.defaultPrevented});if(heldNameTrace.events.length>20)heldNameTrace.events.shift()},true);return true})()`);
  try {
    for(const target of ['apply','pin','row'])for(const ackBeforeRelease of [false,true]) {
      console.log('Held Rename contact', {target,ackBeforeRelease});
      await send({type:'form',action:{type:'new'}});await send({type:'submit',name:`Held ${target} ${ackBeforeRelease}`});const custom=(await view()).id;
      await send({type:'switch',id:original});await send({type:'open',page:'workspaces'});await send({type:'select',id:photo});
      const preview=await layout(),pinBefore=(await view()).switcher.some(row=>row.id===photo);
      await send({type:'form',action:{type:'rename',value:custom}});
      await wait(()=>evaluate('!!document.querySelector(".workspace-name-entry")'),'inline Rename editor');
      await evaluate('document.querySelector(".workspace-name-entry").focus();document.querySelector(".workspace-name-entry").select()');
      const renamed=`Accepted ${target} ${ackBeforeRelease}`;await call('Input.insertText',{text:renamed});
      await storageGate('heldNameTrace.inputs=[];heldNameTrace.events=[];heldName.hold=true;heldName.responses=[];true');
      const selector=target==='apply'?'.workspace-manager footer .suggested-action':`.workspace-row[data-id="${target==='pin'?photo:next}"] .workspace-${target==='pin'?'pin':'choice'}`;
      const at=await point(selector);await evaluate(`window.heldNameControl=document.querySelector(${JSON.stringify(selector)})`);
      await pointer('mousePressed',at);
      await wait(()=>storageGate('heldName.responses.length>0'),'held Rename storage acknowledgement');
      assert.equal((await view()).busy,true,'blur submits Rename while the next native contact remains held');
      assert.equal(await evaluate('heldNameControl.isConnected&&!heldNameControl.disabled'),true,`${target}: a held native control remains enabled until release`);
      assert.deepEqual(await layout(),preview,'Rename publication preserves the selected Photo preview');
      if(ackBeforeRelease){await storageGate('heldName.release();true');await idle();assert.deepEqual(await layout(),preview);}
      await pointer('mouseReleased',at);
      if(!ackBeforeRelease){assert.deepEqual(await layout(),preview,'release queues the follow-up without reverting the preview');await storageGate('heldName.release();true');}
      await idle();assert.equal((await view()).rows.find(row=>row.id===custom)?.title??(await evaluate(`JSON.parse(layerApp.app.workspace_view()).switcher_display.find(row=>row.id===${JSON.stringify(custom)})?.title`)),renamed,'Rename persists before the held follow-up completes');
      if(target==='apply'){assert.equal((await view()).id,photo);assert.equal((await view()).page,null);}
      else {assert.equal((await view()).id,original);assert.equal((await view()).page,'workspaces');assert.equal((await view()).selected,target==='row'?next:photo,await evaluate('JSON.stringify(heldNameTrace)'));if(target==='pin'){assert.equal((await view()).switcher.some(row=>row.id===photo),!pinBefore);assert.deepEqual(await layout(),preview);}await send({type:'dismiss'});}
      await send({type:'switch',id:original});await send({type:'form',action:{type:'delete',value:custom}});await send({type:'submit',name:''});
    }
  } finally {await storageGate('heldName.restore();layerApp.app.workspace_input=heldName.input;true').catch(()=>{});}
  console.log('PASS: held Apply, pin and row contacts survive inline Rename blur before and after storage acknowledgement');
}
