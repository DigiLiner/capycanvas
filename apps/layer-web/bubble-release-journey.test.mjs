import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {createDrawing} from './new-drawing-fixture.mjs';

export async function checkBubbleRelease({call,evaluate,settle}) {
  const directory=process.env.LAYER_TEST_ARTIFACTS??'artifacts/bubble-release/web';
  await mkdir(directory,{recursive:true});
  const failures=[];
  const wait=condition=>evaluate(`new Promise((resolve,reject)=>{const end=performance.now()+12000;function poll(){try{if(${condition})resolve(true);else if(performance.now()>end)reject(Error(${JSON.stringify(condition)}));else setTimeout(poll,30);}catch(error){reject(error)}}poll();})`);
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();};
  const language=async tag=>{await send({type:'restore_saved_settings',saved:JSON.stringify({...await evaluate('layerApp.state().settings'),language:{Explicit:tag}})});await wait(`!layerApp.app.language_pending()&&document.documentElement.lang===${JSON.stringify(tag)}`);};
  const escape=async()=>{for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:'Escape',code:'Escape',windowsVirtualKeyCode:27});await settle();};
  const shot=async name=>writeFile(`${directory}/${name}.png`,Buffer.from((await call('Page.captureScreenshot',{format:'png'})).data,'base64'));
  const check=async(name,run)=>{console.log(`Bubble release: ${name}`);try{await run();console.log(`PASS bubble release: ${name}`);}catch(error){failures.push(`${name}: ${error.message}`);console.error(`FAIL bubble release: ${name}: ${error.message}`);await shot(`${name}-failure`);}};
  const busy='layerApp.documents.busy()||layerApp.state().document_file.busy';
  await wait(`!(${busy})`);
  await evaluate(`(()=>{[...document.querySelectorAll('dialog[open] button')].find(n=>n.textContent==='Keep for Later')?.click()})()`);
  const workspace=await evaluate('layerApp.app.workspace_persistence()');
  await evaluate("layerApp.app.workspace_input(JSON.stringify({type:'switch',id:'builtin:workspace:illustrator'}));null");
  await wait("JSON.parse(layerApp.app.workspace_view()).id==='builtin:workspace:illustrator'&&!JSON.parse(layerApp.app.workspace_view()).busy");
  const fixture=await evaluate('layerApp.app.workspace_persistence()');
  for(const stack of fixture.layout.column_stacks??[]){stack.drawers=false;stack.auto_hide=false;}
  fixture.layout.collapsed=[];
  fixture.layout.bubbles={bubbles:[],rings:[],kept:[]};
  fixture.layout.header={size:'medium',zones:[[{id:1,item:{kind:'capy'}}],[{id:2,item:{kind:'tool',control:{kind:'command',command:'drawing_brush'}}},{id:3,item:{kind:'tool',control:{kind:'panel',panel:'layers'}}}],[]],next_id:4};
  const take=async id=>{
    await evaluate(`(()=>{const viewport=[innerWidth,innerHeight],grab={kind:'header',id:${id}};for(const phase of ['down','move','up'])layerApp.dispatch({type:'bubble',action:{type:'drag',grab,phase,position:[700,420],viewport}})})()`);await settle();
    return evaluate('layerApp.app.bubbles().at(-1).view.id');
  };
  const pixels=selector=>evaluate(`(()=>{const c=document.querySelector(${JSON.stringify(selector)});if(!c)return null;const p=c.getContext('2d',{willReadFrequently:true}).getImageData(0,0,c.width,c.height).data;return {width:c.width,height:c.height,revision:c.dataset.previewRevision,quadrants:[...Array(4)].map((_,q)=>{let opaque=0;for(let y=Math.floor(q/2)*c.height/2;y<(Math.floor(q/2)+1)*c.height/2;y++)for(let x=(q%2)*c.width/2;x<((q%2)+1)*c.width/2;x++)opaque+=p[(y*c.width+x)*4+3]>0;return opaque})}})()`);
  for(const theme of ['light','dark']) {
    await language('en');await send({type:'restore_workspace',workspace:fixture});await send({type:'set_theme',theme});
    for(const [modal,command,selector]of [['new-drawing','new_document','#new-drawing-dialog[open]'],['command-search','search_commands','#command-bar[open]']]) {
      await check(`${theme}-${modal}-cold-thumbnails`,async()=>{
        await wait(`!(${busy})&&layerApp.state().commands.find(c=>c.id==='new_document').enabled`);
        await send({type:'invoke',command:'new_document'});
        await wait("!!document.querySelector('#new-drawing-dialog[open]')");
        await evaluate(`(async()=>{const epoch=layerApp.state().document_file.epoch,end=performance.now()+20000;await ${createDrawing({width:320,height:240})};while(layerApp.documents.busy()||layerApp.state().document_file.epoch===epoch){if(performance.now()>end)throw Error('New drawing adoption did not finish');await new Promise(resolve=>setTimeout(resolve,0))}window.releaseThumbnailCalls=[];window.releaseThumbnailReplies=[];window.releaseThumbnailTakes=0;const canvases=new WeakMap();let canvasId=0;window.releaseThumbnailCanvases=()=>[...document.querySelectorAll('#layer-rows .layer-thumbnail:not(.layer-texture) > canvas')].map(c=>{if(!canvases.has(c))canvases.set(c,++canvasId);return{id:canvases.get(c),layer:c.closest('[data-layer]')?.dataset.layer,connected:c.isConnected,visible:c.getBoundingClientRect().height,revision:c.dataset.previewRevision??null}});window.releaseThumbnailOriginal=layerApp.app.request_layer_thumbnail;window.releaseThumbnailTakeOriginal=layerApp.app.take_layer_thumbnail;layerApp.app.request_layer_thumbnail=function(...args){const accepted=releaseThumbnailOriginal.apply(this,args);releaseThumbnailCalls.push({accepted,request:String(args[0]),at:performance.now(),target:String(args[1]),canvases:releaseThumbnailCanvases()});return accepted};layerApp.app.take_layer_thumbnail=function(...args){releaseThumbnailTakes++;const image=releaseThumbnailTakeOriginal.apply(this,args);if(image)releaseThumbnailReplies.push({request:String(image[0]),extent:[image[1],image[2]],at:performance.now(),canvases:releaseThumbnailCanvases()});return image};layerApp.dispatch({type:'invoke',command:${JSON.stringify(command)}})})()`);
        await wait(`!!document.querySelector(${JSON.stringify(selector)})`);
        const cold=await evaluate(`[...document.querySelectorAll('#layer-rows .layer-thumbnail:not(.layer-texture) > canvas')].filter(c=>!c.closest('[hidden]')).some(c=>!c.dataset.previewRevision?.startsWith(String(layerApp.state().document_file.epoch)+':'))`);
        assert.ok(cold,'new drawing opens the modal before its layer previews arrive');
        try {await wait(`[...document.querySelectorAll('#layer-rows .layer-thumbnail:not(.layer-texture) > canvas')].filter(c=>!c.closest('[hidden]')).every(c=>c.dataset.previewRevision?.startsWith(String(layerApp.state().document_file.epoch)+':'))`);}
        catch(error){const detail=await evaluate(`JSON.parse(JSON.stringify({calls:releaseThumbnailCalls,takes:releaseThumbnailTakes,replies:releaseThumbnailReplies,canvases:releaseThumbnailCanvases(),settingsOpen:layerApp.state().settings_open,panels:[...document.querySelectorAll('.layers-panel')].map(p=>({connected:p.isConnected,height:p.clientHeight})),session:layerApp.app.gpu_diagnostics().session,canvasWork:layerApp.app.canvas_work_pending(),shaders:layerApp.app.shader_work_pending(true),startup:layerApp.startupTimes,modal:!!document.querySelector(${JSON.stringify(selector)})},(_,v)=>typeof v==='bigint'?String(v):v))`);await writeFile(`${directory}/${theme}-${modal}-diagnostics.json`,JSON.stringify(detail,null,2));throw Error(`${error.message}; diagnostics ${JSON.stringify({...detail,calls:{total:detail.calls.length,accepted:detail.calls.filter(c=>c.accepted).length}})}`);}
        assert.ok(await evaluate(`!!document.querySelector(${JSON.stringify(selector)})`),'thumbnail delivery keeps the modal open');await shot(`${theme}-${modal}`);
      });
      await escape();await evaluate(`if(window.releaseThumbnailOriginal){layerApp.app.request_layer_thumbnail=releaseThumbnailOriginal;layerApp.app.take_layer_thumbnail=releaseThumbnailTakeOriginal;delete window.releaseThumbnailOriginal;delete window.releaseThumbnailTakeOriginal}`);await wait(`!(${busy})`);
    }
    await check(`${theme}-masked-layer-menu`,async()=>{
      const id=await evaluate('Number(layerApp.state().layers.find(l=>l.editing).id)');
      await send({type:'layer',action:{op:'add_mask',id,replace:false}});await send({type:'layer',action:{op:'select',id,mask:true}});
      await evaluate("document.querySelector('.layer-more').click()");await settle();
      await wait("!!document.querySelector('.panel-context-menu:popover-open')");
      await language('ja');await settle();
      assert.ok(await evaluate("!!document.querySelector('.panel-context-menu:popover-open')"),'masked layer menu stays open across frames and language publication');
      assert.ok(await evaluate('layerApp.state().layer_tools.editing_layer.mask_selected'),'language publication retains the mask target');
      assert.notEqual(await evaluate('document.body.dataset.gpu'),'unavailable');await shot(`${theme}-mask-menu-ja`);
    });
    await escape();await language('en');
    await check(`${theme}-panel-menu-section`,async()=>{
      await evaluate(`(()=>{const n=document.querySelector('[data-panel="sizes"] .panel-grip')??document.querySelector('[data-panel="sizes"]');const r=n.getBoundingClientRect();n.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:r.x+10,clientY:r.y+10}))})()`);await settle();
      assert.ok(await evaluate("!!document.querySelector('.panel-context-menu:popover-open')"));
      assert.notEqual(await evaluate("document.querySelector('.panel-context-menu:popover-open').firstElementChild.tagName"),'HR','the first rendered menu section contains an item');
    });
    await escape();
    await check(`${theme}-chooser-tags`,async()=>{
      const id=await take(2);
      for(const tag of ['en','ja']) {
        await language(tag);await evaluate('document.fonts.ready');await settle();
        for(let attempt=0;attempt<3;attempt++){if(await evaluate(`layerApp.app.bubbles().find(p=>p.view.id===${id}).view.chooser.some(item=>item.kind==='medium')`))break;await send({type:'bubble',action:{type:'tap',id,position:[0,0]}});}
        await wait(`layerApp.app.bubbles().find(p=>p.view.id===${id}).view.chooser.some(item=>item.kind==='medium')`);
        await evaluate('new Promise(resolve=>setTimeout(resolve,500))');await settle();
        const tags=await evaluate(`(()=>{const root=document.querySelector('.tool-bubble[data-bubble="${id}"]');return [...root.querySelectorAll('.bubble-tag:not([hidden])')].map(n=>{const span=n.firstElementChild,range=document.createRange();range.selectNodeContents(span);return{text:n.textContent,available:span.getBoundingClientRect().width,natural:range.getBoundingClientRect().width,width:n.getBoundingClientRect().width}})})()`);
        assert.ok(tags.length,'chooser renders medium name tags');
        for(const text of tags)assert.ok(text.available+.05>=Math.min(text.natural,78),`${tag} ${text.text} fits its intrinsic text: ${JSON.stringify(text)}`);
        await shot(`${theme}-chooser-${tag}`);
      }
      await send({type:'bubble',action:{type:'put_back',id}});
    });
    await language('en');
    await check(`${theme}-folded-layers-thumbnail`,async()=>{
      const id=await take(3);await send({type:'bubble',action:{type:'close',id}});
      const selector=`.tool-bubble[data-bubble="${id}"] .bubble-layer-thumbnail`;
      await wait(`!!document.querySelector(${JSON.stringify(selector)})?.dataset.previewRevision`);
      const p=await pixels(selector);assert.ok(p.quadrants.every(n=>n===p.width*p.height/4),`folded thumbnail fills every quadrant: ${JSON.stringify(p)}`);
      await evaluate(`window.releaseFoldedCanvas=document.querySelector(${JSON.stringify(selector)});window.releaseFoldedPixels=releaseFoldedCanvas.toDataURL()`);
      for(const selectedTheme of [theme,theme==='light'?'dark':'light',theme]) {
        await send({type:'set_theme',theme:selectedTheme});
        assert.ok(await evaluate(`document.querySelector(${JSON.stringify(selector)})===releaseFoldedCanvas&&releaseFoldedCanvas.toDataURL()===releaseFoldedPixels`),'theme publication retains the thumbnail canvas and its pixels');
        await wait(`!!document.querySelector(${JSON.stringify(selector)})?.dataset.previewRevision`);
        await shot(`${theme}-folded-layers-${selectedTheme}`);
      }
      await send({type:'bubble',action:{type:'put_back',id}});
    });
    await check(`${theme}-zen-toolbar-and-edge-peek`,async()=>{
      await send({type:'restore_workspace',workspace:fixture});
      const group=await evaluate("layerApp.app.layout(innerWidth,innerHeight).groups.find(g=>g.panels.includes('toolbar')).id"),selector=`.dock-group[data-group="${group}"]`;
      await send({type:'customize',action:{type:'set_show_in_zen',panel:'toolbar',shown:true}});
      const camera=await evaluate('JSON.stringify(layerApp.state().camera,(_,v)=>typeof v==="bigint"?String(v):v)');
      await send({type:'invoke',command:'zen_mode'});
      await wait(`document.querySelector(${JSON.stringify(selector)}).classList.contains('zen-kept')&&Number(getComputedStyle(document.querySelector(${JSON.stringify(selector)})).opacity)>.99`);
      assert.ok(await evaluate("document.querySelector('#workspace').classList.contains('zen-hidden')"),'kept toolbar stays visible in Zen');
      await shot(`${theme}-zen-kept-toolbar`);
      await send({type:'customize',action:{type:'set_show_in_zen',panel:'toolbar',shown:false}});
      await wait(`Number(getComputedStyle(document.querySelector(${JSON.stringify(selector)})).opacity)<.01`);
      await call('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:5});
      const y=await evaluate('innerHeight/2');
      for(const [type,x]of [['touchStart',1],['touchMove',24],['touchMove',65],['touchMove',90]]){await call('Input.dispatchTouchEvent',{type,touchPoints:[{id:1,x,y}]});await settle();}
      await call('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});await settle();
      await wait(`document.querySelector(${JSON.stringify(selector)}).classList.contains('zen-kept')&&Number(getComputedStyle(document.querySelector(${JSON.stringify(selector)})).opacity)>.99`);
      assert.equal(await evaluate('JSON.stringify(layerApp.state().camera,(_,v)=>typeof v==="bigint"?String(v):v)'),camera,'edge peek preserves the canvas camera');
      await shot(`${theme}-zen-edge-peek`);
      await send({type:'invoke',command:'zen_mode'});
      await call('Emulation.setTouchEmulationEnabled',{enabled:false});
    });
    if(await evaluate('layerApp.state().workspace.zen_mode'))await send({type:'invoke',command:'zen_mode'});
    await call('Emulation.setTouchEmulationEnabled',{enabled:false});
  }
  await send({type:'restore_workspace',workspace});await language('en');
  assert.deepEqual(failures,[]);
}

export async function checkBubbleReleaseColdStartup({call,evaluate,settle}) {
  const directory=process.env.LAYER_TEST_ARTIFACTS??'artifacts/bubble-release/web';await mkdir(directory,{recursive:true});
  const wait=condition=>evaluate(`new Promise((resolve,reject)=>{const end=performance.now()+20000;function poll(){try{if(${condition})resolve(true);else if(performance.now()>end)reject(Error(${JSON.stringify(condition)}));else setTimeout(poll,20);}catch(error){reject(error)}}poll();})`);
  const reports=[],failures=[];
  for(const theme of ['light','dark'])for(const [name,command,selector]of [['new-drawing','new_document','#new-drawing-dialog[open]'],['command-search','search_commands','#command-bar[open]']]) {
    const source=`(()=>{const timer=setInterval(()=>{if(!window.layerApp)return;if(!window.releaseColdTrace){window.releaseColdTrace={requests:[],takes:0,replies:[]};const request=layerApp.app.request_layer_thumbnail,take=layerApp.app.take_layer_thumbnail;layerApp.app.request_layer_thumbnail=function(...args){const accepted=request.apply(this,args);releaseColdTrace.requests.push({request:String(args[0]),target:String(args[1]),accepted,at:performance.now()});return accepted};layerApp.app.take_layer_thumbnail=function(...args){releaseColdTrace.takes++;const image=take.apply(this,args);if(image)releaseColdTrace.replies.push({request:String(image[0]),at:performance.now()});return image};}if(!layerApp.app.canvas_presented()||!layerApp.state().commands.find(c=>c.id===${JSON.stringify(command)})?.enabled)return;clearInterval(timer);const workspace=layerApp.app.workspace_persistence();workspace.layout.collapsed=[];for(const stack of workspace.layout.column_stacks??[]){stack.drawers=false;stack.auto_hide=false;}layerApp.dispatch({type:'restore_workspace',workspace});layerApp.dispatch({type:'set_theme',theme:${JSON.stringify(theme)}});window.releaseColdStartup={at:performance.now(),brushReady:layerApp.app.brush_ready(),startup:structuredClone(layerApp.startupTimes),previews:[...document.querySelectorAll('#layer-rows .layer-thumbnail:not(.layer-texture) > canvas')].filter(c=>!c.closest('[hidden]')).map(c=>c.dataset.previewRevision??null)};layerApp.dispatch({type:'invoke',command:${JSON.stringify(command)}});},0)})()`;
    const {identifier}=await call('Page.addScriptToEvaluateOnNewDocument',{source});
    try {
      for(const type of ['mousePressed','mouseReleased'])await call('Input.dispatchMouseEvent',{type,x:1,y:1,button:'left',buttons:type==='mousePressed'?1:0,clickCount:1});
      await call('Page.reload',{ignoreCache:true});await new Promise(resolve=>setTimeout(resolve,1000));await wait('!!window.releaseColdStartup');await wait(`!!document.querySelector(${JSON.stringify(selector)})`);
      const before=await evaluate('releaseColdStartup');
      assert.ok(before.previews.length&&before.previews.some(revision=>revision===null),'startup modal opens before a visible preview arrives');
      await wait("[...document.querySelectorAll('#layer-rows .layer-thumbnail:not(.layer-texture) > canvas')].filter(c=>!c.closest('[hidden]')).every(c=>c.dataset.previewRevision?.startsWith(String(layerApp.state().document_file.epoch)+':'))");
      assert.ok(await evaluate(`!!document.querySelector(${JSON.stringify(selector)})`),'cold thumbnail delivery keeps the modal open');
      const elapsed=await evaluate('performance.now()-releaseColdStartup.at');reports.push({theme,modal:name,...before,elapsed});
      await settle();await writeFile(`${directory}/cold-startup-${theme}-${name}.png`,Buffer.from((await call('Page.captureScreenshot',{format:'png'})).data,'base64'));
      console.log(`PASS cold startup ${theme}/${name}: opened before brush ready=${!before.brushReady}, thumbnails after ${Math.round(elapsed)}ms`);
    } catch(error) {
      const detail=await evaluate(`JSON.parse(JSON.stringify({before:releaseColdStartup,trace:releaseColdTrace,session:layerApp.app.gpu_diagnostics().session,startup:layerApp.startupTimes,brushReady:layerApp.app.brush_ready(),canvasWork:layerApp.app.canvas_work_pending(),shaders:layerApp.app.shader_work_pending(true),requiredShaders:layerApp.app.shader_work_pending(false),previews:[...document.querySelectorAll('#layer-rows .layer-thumbnail:not(.layer-texture) > canvas')].map(c=>({layer:c.closest('[data-layer]').dataset.layer,hidden:!!c.closest('[hidden]'),height:c.getBoundingClientRect().height,revision:c.dataset.previewRevision??null})),settingsOpen:layerApp.state().settings_open},(_,v)=>typeof v==='bigint'?String(v):v))`);
      await writeFile(`${directory}/cold-startup-${theme}-${name}-failure.json`,JSON.stringify(detail,null,2));
      console.error('Cold startup failure:',JSON.stringify({...detail,trace:{requests:detail.trace.requests.length,accepted:detail.trace.requests.filter(r=>r.accepted).length,takes:detail.trace.takes,replies:detail.trace.replies}}));failures.push(`${theme}/${name}: ${error.message}`);
    } finally {await call('Page.removeScriptToEvaluateOnNewDocument',{identifier});}
    for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:'Escape',code:'Escape',windowsVirtualKeyCode:27});await settle();
  }
  await writeFile(`${directory}/cold-startup.json`,JSON.stringify(reports,null,2));
  assert.deepEqual(failures,[]);
}
