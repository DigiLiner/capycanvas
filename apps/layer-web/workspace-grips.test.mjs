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
  const key=async name=>{const code={Enter:13,' ':32,Escape:27}[name];for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:name,code:name===' '?'Space':name,windowsVirtualKeyCode:code,...(type==='keyDown'&&name!=='Escape'?{text:name==='Enter'?'\r':' ',unmodifiedText:name==='Enter'?'\r':' '}:{})});await idle();};
  const menuOpen=()=>evaluate('document.querySelector(".workspace-row-menu").matches(":popover-open")');
  const pins=async()=>new Set((await view()).switcher.map(row=>row.id));
  const p='builtin:workspace:painter',f='builtin:workspace:photographer';
  const output=process.env.LAYER_TEST_ARTIFACTS||'artifacts/workspace-grips';await mkdir(output,{recursive:true});
  await idle();const original=await view();
  await evaluate(`window.workspaceGripEvents=[];const record=(e,phase)=>{workspaceGripEvents.push({type:e.type,phase,key:e.key,target:e.target.closest?.("button")?.className,same:e.target===window.retainedKeyboardGrip,prevented:e.defaultPrevented});if(workspaceGripEvents.length>30)workspaceGripEvents.shift()};for(const type of ["pointerdown","pointerup","click"])document.addEventListener(type,e=>record(e,"capture"),true);for(const type of ["keydown","keypress","keyup"])window.addEventListener(type,e=>record(e,"window-bubble"));`);
  for(const theme of ['dark','light']) {
    await evaluate(`layerApp.dispatch({type:'set_theme',theme:${JSON.stringify(theme)}});null`);await idle();
    await send({type:'open',page:'workspaces'});
    const creation=await evaluate('(()=>{const add=document.querySelector(".workspace-add"),a=add.getBoundingClientRect(),l=document.querySelector(".workspace-list").getBoundingClientRect();return {label:add.textContent.trim(),right:a.right,listRight:l.right,bottom:a.bottom,listTop:l.top,header:!!add.closest(".dialog-header")}})()');
    assert.match(creation.label,/^New Workspace(?:…)?$/i);assert.equal(creation.header,false);assert.ok(Math.abs(creation.right-creation.listRight)<.5);assert.ok(creation.bottom<=creation.listTop);
    await click('.workspace-add');await evaluate('document.querySelector(".workspace-form input").focus();document.querySelector(".workspace-form input").select()');await call('Input.insertText',{text:`Grip study ${theme}`});await click('.workspace-form .suggested-action');const custom=(await view()).id;
    await send({type:'switch',id:original.id});await send({type:'open',page:'workspaces'});await click(`${row(f)} .workspace-choice`);
    const preview=await layout(),savedPins=await pins(),savedOrder=(await view()).order;
    for(const item of (await view()).rows)assert.equal(await evaluate(`!!document.querySelector(${JSON.stringify(`${row(item.id)} .workspace-options`)})`),item.id===custom,'only custom rows expose options dots');
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
    await click(`${row(custom)} .workspace-options`);assert.ok(await evaluate('document.querySelector(".workspace-row-menu [data-action=rename]")&&!document.querySelector(".workspace-row-menu [data-action=rename]").disabled'));assert.ok(await evaluate('document.querySelector(".workspace-row-menu [data-action=delete]")&&!document.querySelector(".workspace-row-menu [data-action=delete]").disabled'));
    await click('.workspace-row-menu [data-action="rename"]');await evaluate('document.querySelector(".workspace-form input").focus();document.querySelector(".workspace-form input").select()');await call('Input.insertText',{text:`Renamed grip study ${theme}`});await click('.workspace-form .suggested-action');assert.equal((await view()).rows.find(item=>item.id===custom).title,`Renamed grip study ${theme}`);await unchanged();
    const shot=await call('Page.captureScreenshot',{format:'png'});await writeFile(`${output}/${theme}.png`,Buffer.from(shot.data,'base64'));
    await click(`${row(custom)} .workspace-options`);await click('.workspace-row-menu [data-action="delete"]');await click('.workspace-form .suggested-action');assert.equal((await view()).rows.some(item=>item.id===custom),false);assert.equal((await view()).selected,f);assert.deepEqual(await layout(),preview);
    await click('.workspace-manager footer button');await idle();
  }
  console.log('PASS: built-in grips provide pointer and keyboard ordering without dots, immediate drags preserve preview/pins, and custom dots retain Rename/Delete in both themes');
}
