import assert from "node:assert/strict";
import {mkdir,writeFile} from "node:fs/promises";

// Match native_workspace_manager_visual: fresh storage, three defaults, scale 1.
export async function checkWorkspaceManagerVisual({call,evaluate,settle}) {
  const output=process.env.LAYER_TEST_ARTIFACTS || 'artifacts/workspace-manager-visual/web';
  await mkdir(output,{recursive:true});
  const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  const row='.workspace-row[data-id="builtin:workspace:painter"]';
  const move=async selector=>{
    const point=selector?await evaluate(`(()=>{const r=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};})()`):{x:10,y:10};
    await call('Input.dispatchMouseEvent',{type:'mouseMoved',...point});await pause(200);await settle();
  };
  for(const theme of ['dark','light']) {
    await evaluate(`layerApp.dispatch({type:'set_theme',theme:${JSON.stringify(theme)}});layerApp.app.workspace_input(JSON.stringify({type:'open',page:'workspaces'}));null`);
    await pause(700);await settle();
    assert.equal(await evaluate('document.querySelector(".workspace-manager").open'),true);
    const samples={};
    for(const [state,selector] of [['normal',null],['row',`${row} .workspace-choice`],['grip',`${row} .workspace-grip`],['pin',`${row} .workspace-pin`],['current','.workspace-row[data-id="builtin:workspace:illustrator"] .workspace-choice'],['new','.workspace-row-actions .workspace-add'],['cancel','.workspace-manager footer button'],['disabled','.workspace-manager .suggested-action']]) {
      await move(selector);
      const shot=await call('Page.captureScreenshot',{format:'png'});
      await writeFile(`${output}/web-${theme}-${state}.png`,Buffer.from(shot.data,'base64'));
      const geometry=await evaluate(`(()=>{function record(e){const r=e.getBoundingClientRect(),s=getComputedStyle(e);return {tag:e.tagName,css:e.className,text:e.children.length?null:e.textContent,bounds:[r.x,r.y,r.width,r.height],hover:e.matches(':hover'),background:s.backgroundColor,color:s.color,font:s.font,radius:s.borderRadius,cornerShape:s.cornerShape,opacity:s.opacity,children:[...e.children].map(record)};}return record(document.querySelector('.workspace-manager'));})()`);
      await writeFile(`${output}/web-${theme}-${state}.json`,JSON.stringify(geometry,null,2));
      samples[state]=await evaluate(`(()=>{const row=document.querySelector(${JSON.stringify(row)}),background=e=>getComputedStyle(e).backgroundColor;return {row:background(row),choice:background(row.querySelector('.workspace-choice')),grip:background(row.querySelector('.workspace-grip')),pin:background(row.querySelector('.workspace-pin')),primary:background(document.querySelector('.workspace-manager .suggested-action'))};})()`);
    }
    const normal=samples.normal;
    assert.notEqual(samples.row.row,normal.row,'hover highlights the whole row');
    for(const state of ['grip','pin']) assert.equal(samples[state].row,samples.row.row,`${state} shares the row hover surface`);
    for(const sample of Object.values(samples)) {
      assert.equal(sample.choice,'rgba(0, 0, 0, 0)','row text never gets a separate button background');
      assert.equal(sample.grip,'rgba(0, 0, 0, 0)','the handle never gets a button background');
      assert.equal(sample.pin,'rgba(0, 0, 0, 0)','the pin stays unboxed while pinned, hovered and focused');
      assert.equal(sample.primary,normal.primary,'hovering disabled confirmation preserves its color');
    }
    assert.equal(await evaluate('[...document.querySelectorAll(".workspace-row")].every(row=>!row.querySelector(".workspace-options"))'),true,'built-in rows have no extra options button');
    const measured=await evaluate(`(()=>{const node=s=>document.querySelector(s),rect=s=>node(s).getBoundingClientRect(),d=rect('.workspace-manager'),h=rect('.workspace-manager .dialog-header'),l=rect('.workspace-list'),a=rect('.workspace-row-actions .workspace-add'),trash=rect('.workspace-delete'),f=rect('.workspace-manager footer'),buttons=[...node('.workspace-manager footer').querySelectorAll('button')].filter(n=>!n.hidden),last=buttons.at(-1).getBoundingClientRect(),probe=document.createElement('i');probe.style.cssText='position:absolute;visibility:hidden;pointer-events:none;width:var(--control-radius);height:0';node('.workspace-manager').append(probe);const sharedControlRadius=probe.getBoundingClientRect().width;probe.remove();return {sharedControlRadius,padding:[l.left-d.left,d.right-l.right,d.bottom-f.bottom,getComputedStyle(node('.workspace-manager')).paddingTop],sections:[l.top-h.bottom,f.top-a.bottom],actionGap:a.top-l.bottom,actionSizes:[a.width,a.height,trash.width,trash.height],listHeight:l.height,rowHeights:[...node('.workspace-list').children].map(n=>n.getBoundingClientRect().height),rowBorders:[...node('.workspace-list').children].map(n=>getComputedStyle(n).borderBottomWidth),buttonRight:last.right,listRight:l.right,footerButtonWidths:buttons.map(n=>n.getBoundingClientRect().width),footerWidth:f.width,radius:getComputedStyle(node('.workspace-list')).borderRadius,focus:document.activeElement.matches('.workspace-choice[aria-selected="true"]'),extra:[...document.querySelectorAll('.workspace-intro,.workspace-search,.workspace-options')].filter(n=>n.getClientRects().length).length};})()`);
    assert.deepEqual(measured.padding.slice(0,3),[20,20,20],'manager body uses20 px outer margins');
    assert.deepEqual(measured.sections,[20,12],'header has20 px body inset and footer has12 px section spacing');assert.equal(measured.actionGap,6);
    assert.deepEqual(measured.actionSizes,[34,34,34,34]);assert.ok(measured.listHeight<=7*56);assert.ok(measured.rowHeights.every(height=>height===56));assert.ok(measured.rowBorders.every(width=>width==='0px'),'rows have no bottom rule');
    assert.equal(measured.buttonRight,measured.listRight,'footer aligns natural buttons to the right');assert.ok(measured.footerButtonWidths.reduce((a,b)=>a+b,0)<measured.footerWidth);assert.ok(measured.sharedControlRadius>0);assert.equal(parseFloat(measured.radius),measured.sharedControlRadius,'list clip uses the shared control radius');assert.equal(measured.extra,0);assert.equal(measured.focus,true);
    await writeFile(`${output}/web-${theme}-measurements.json`,JSON.stringify(measured,null,2));
    assert.equal(await evaluate('document.querySelector(".workspace-row-actions .workspace-add").textContent.trim()'),'','New Workspace uses an icon');
    await evaluate('document.querySelector(".workspace-row-actions .workspace-add").focus()');
    await evaluate(`document.querySelector(${JSON.stringify(`${row} .workspace-choice`)}).focus()`);
    assert.equal(await evaluate(`getComputedStyle(document.querySelector(${JSON.stringify(row)})).outlineStyle`),'solid');
    // Touch mode must not retain a mouse hover while the list scrolls or drags.
    await move(`${row} .workspace-grip`);await evaluate('document.documentElement.dataset.touch=""');await settle();
    assert.equal(await evaluate(`getComputedStyle(document.querySelector(${JSON.stringify(row)})).backgroundColor`),normal.row);
    await evaluate('delete document.documentElement.dataset.touch');
    const before=await evaluate("JSON.parse(layerApp.app.workspace_view()).selected");
    await evaluate(`window.retainedManagerPin=document.querySelector(${JSON.stringify(`${row} .workspace-pin`)})`);
    await move(`${row} .workspace-pin`);
    const point=await evaluate(`(()=>{const r=document.querySelector(${JSON.stringify(`${row} .workspace-pin`)}).getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};})()`);
    await call('Input.dispatchMouseEvent',{type:'mousePressed',...point,button:'left',buttons:1,clickCount:1});
    await call('Input.dispatchMouseEvent',{type:'mouseReleased',...point,button:'left',buttons:0,clickCount:1});
    await pause(500);await settle();
    assert.deepEqual(await evaluate('(()=>{const v=JSON.parse(layerApp.app.workspace_view());return [v.id,v.selected,v.enabled]})()'),['builtin:workspace:illustrator',before,false],'clicking the pin preserves preview selection');
    assert.equal(await evaluate('retainedManagerPin.isConnected&&retainedManagerPin.getAttribute("aria-pressed")==="false"'),true);
    assert.equal(await evaluate('getComputedStyle(retainedManagerPin).backgroundColor'),'rgba(0, 0, 0, 0)','the unpinned icon has no button background');
    await evaluate('retainedManagerPin.focus()');
    for(const type of ['keyDown','keyUp']) await call('Input.dispatchKeyEvent',{type,key:'Enter',code:'Enter',windowsVirtualKeyCode:13,...(type==='keyDown'?{text:'\r',unmodifiedText:'\r'}:{})});
    await pause(500);await settle();
    assert.equal(await evaluate('document.activeElement===retainedManagerPin&&retainedManagerPin.getAttribute("aria-pressed")==="true"'),true,'keyboard pin retains focus');
    await evaluate(`document.querySelector(${JSON.stringify(`${row} .workspace-choice`)}).click()`);await pause(500);await settle();
    await move('.workspace-manager .suggested-action');
    const background=await evaluate(`getComputedStyle(document.querySelector('.workspace-manager .suggested-action')).backgroundColor`);
    const color=background.match(/[\d.]+/g).map(Number);
    assert.ok(color[2]>color[1] && color[1]>color[0] && (color[3]??1)===1,`enabled confirmation remains blue on hover: ${background}`);
    await evaluate(`document.querySelector('.workspace-manager footer button').click()`);await pause(300);
  }
  console.log(`PASS: GTK manager geometry, row/button hover, disabled styling and keyboard focus in both themes. Captures: ${output}`);
}
