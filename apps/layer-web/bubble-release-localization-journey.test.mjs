import assert from 'node:assert/strict';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {createDrawing} from './new-drawing-fixture.mjs';

export async function checkBubbleReleaseLocalization({call,evaluate,settle}) {
  const directory=process.env.LAYER_TEST_ARTIFACTS??'artifacts/bubble-release/localization';
  await mkdir(directory,{recursive:true});
  const failures=[],measurements=[],tags=await evaluate('layerApp.app.bootstrap_view().shipped_tags'),copy={};
  for(const tag of tags) {
    copy[tag]={};
    for(const domain of ['common','workspace','settings']) {
      const source=await readFile(new URL(`../../assets/locales/${tag}/${domain}.ftl`,import.meta.url),'utf8');
      for(const line of source.split('\n')) {const match=line.match(/^([\w-]+) = (.*)$/);if(match)copy[tag][match[1]]=match[2];}
    }
  }
  const wait=condition=>evaluate(`new Promise((resolve,reject)=>{const end=performance.now()+30000;function poll(){try{if(${condition})resolve(true);else if(performance.now()>end)reject(Error(${JSON.stringify(condition)}));else setTimeout(poll,25)}catch(error){reject(error)}}poll()})`);
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();};
  const language=async tag=>{await send({type:'restore_saved_settings',saved:await evaluate(`JSON.stringify({...layerApp.state().settings,language:{Explicit:${JSON.stringify(tag)}}})`)});try{await wait(`!layerApp.app.language_pending()&&document.documentElement.lang===${JSON.stringify(tag)}`);}catch(error){await writeFile(`${directory}/language-${tag}-failure.local.json`,JSON.stringify(await evaluate(`({pending:layerApp.app.language_pending(),tag:document.documentElement.lang,active:document.activeElement?.outerHTML,documentBusy:layerApp.documents.busy(),interactionBusy:layerApp.documents.interactionBusy()})`),null,2));throw error;}await evaluate('document.fonts.ready');await settle();};
  const capture=async name=>writeFile(`${directory}/${name}.png`,Buffer.from((await call('Page.captureScreenshot',{format:'png'})).data,'base64'));
  const check=async(name,run)=>{try{await run();console.log(`PASS release localization: ${name}`);}catch(error){failures.push(`${name}: ${error.message}`);console.error(`FAIL release localization: ${name}: ${error.message}`);await capture(`${name}-failure`);}};
  const escape=async()=>{for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:'Escape',code:'Escape',windowsVirtualKeyCode:27});await settle();};
  const clean=text=>text.replace(/[\u2068\u2069]/g,'');
  const geometry=async(selector,{selects=false}={})=>evaluate(`(()=>{const root=document.querySelector(${JSON.stringify(selector)});if(!root)throw Error('Missing layout root '+${JSON.stringify(selector)});return [...root.querySelectorAll(${JSON.stringify(selects?'select':'button')})].filter(n=>n.checkVisibility()).map(n=>{const r=n.getBoundingClientRect(),s=getComputedStyle(n),sample=document.createElement('span');sample.textContent=n.tagName==='SELECT'?n.selectedOptions[0]?.textContent:n.textContent;sample.style.cssText='position:fixed;visibility:hidden;white-space:pre';sample.style.font=s.font;document.body.append(sample);const natural=sample.getBoundingClientRect().width;sample.remove();return{text:n.tagName==='SELECT'?n.selectedOptions[0]?.textContent:n.textContent,width:r.width,height:r.height,natural,available:n.clientWidth-parseFloat(s.paddingLeft)-parseFloat(s.paddingRight),whiteSpace:s.whiteSpace,scroll:n.scrollWidth-n.clientWidth}})})()`);
  const fullButtons=async(name,selector)=>{const rows=await geometry(selector);assert.ok(rows.length,`${name} has buttons`);for(const row of rows.filter(row=>row.text))assert.ok(row.available+.6>=row.natural&&row.scroll<=1,`${name} complete button: ${JSON.stringify(row)}`);measurements.push({name,rows});};
  const fullSelects=async(name,selector)=>{const rows=await geometry(selector,{selects:true});assert.ok(rows.length,`${name} has choices`);for(const row of rows)assert.ok(row.available-16+.6>=row.natural,`${name} complete selected choice: ${JSON.stringify(row)}`);measurements.push({name,rows});};
  await wait('!layerApp.documents.busy()&&!layerApp.state().document_file.busy');
  const original=await evaluate('layerApp.app.workspace_persistence()');
  await language('en');await evaluate(`layerApp.app.workspace_input(JSON.stringify({type:'switch',id:'builtin:workspace:painter'}));null`);await wait(`JSON.parse(layerApp.app.workspace_view()).id==='builtin:workspace:painter'&&!JSON.parse(layerApp.app.workspace_view()).busy`);
  await send({type:'invoke',command:'new_document'});await evaluate(createDrawing({width:320,height:240}));await wait('!layerApp.documents.busy()&&!layerApp.state().document_file.busy');
  for(const theme of ['light','dark']) {
    await language('en');await send({type:'set_theme',theme});
    await check(`${theme}-configure-live`,async()=>{
      for(const panel of ['sizes','toolbar']) {
        await send({type:'customize',action:{type:'set_panel_visible',panel,visible:true}});await send({type:'customize',action:{type:'show_all_controls',panel}});await wait(`!!document.querySelector('.panel-configuration h3')`);
        const english=await evaluate(`layerApp.app.panel_view(${JSON.stringify(panel)})`);
        await evaluate(`window.releaseConfiguration=document.querySelector('.panel-configuration');window.releaseConfigurationChecks=[...releaseConfiguration.querySelectorAll('[data-visible]')];`);
        for(const tag of ['de','ja']) {
          await language(tag);
          const state=await evaluate(`(()=>{const view=layerApp.app.panel_view(${JSON.stringify(panel)});return{view,same:releaseConfiguration===document.querySelector('.panel-configuration')&&releaseConfigurationChecks.every((n,i)=>n===releaseConfiguration.querySelectorAll('[data-visible]')[i]),title:releaseConfiguration.querySelector('h3').textContent,hint:releaseConfiguration.querySelector('p').textContent,labels:[...releaseConfiguration.querySelectorAll('[data-visible]')].map(n=>n.parentElement.lastChild.textContent)}})()`);
          assert.ok(state.same,`${tag} Configure retains controls`);assert.equal(state.title,state.view.configuration_title);assert.equal(state.hint,state.view.configuration_hint);assert.deepEqual(state.labels,state.view.controls.map(row=>row.label));assert.notEqual(state.title,english.configuration_title);assert.notEqual(state.hint,english.configuration_hint);if(state.labels.length)assert.notDeepEqual(state.labels,english.controls.map(row=>row.label));
          if(panel==='toolbar')assert.equal(clean(state.title),copy[tag]['workspace-configure-title'].replace('{ $name }',copy[tag]['workspace-tools-toolbar']));
          const diameter=await evaluate(`(()=>{window.releaseCaptionNodes=[releaseConfiguration.querySelector('h3').firstChild,releaseConfiguration.querySelector('p').firstChild,...releaseConfigurationChecks.map(n=>n.parentElement.lastChild)];window.releaseCaptionText=releaseCaptionNodes.map(n=>n.textContent);window.releaseCaptionMutations=[];window.releaseCaptionObserver=new MutationObserver(records=>releaseCaptionMutations.push(...records));for(const node of releaseCaptionNodes)releaseCaptionObserver.observe(node.parentElement,{childList:true,characterData:true,subtree:true});return layerApp.state().brush.diameter})()`);
          await send({type:'set_brush_size',value:diameter+1});assert.equal(await evaluate('layerApp.state().brush.diameter'),diameter+1);
          await send({type:'set_brush_size',value:diameter});
          const retained=await evaluate(`(()=>{releaseCaptionObserver.disconnect();const nodes=[releaseConfiguration.querySelector('h3').firstChild,releaseConfiguration.querySelector('p').firstChild,...releaseConfigurationChecks.map(n=>n.parentElement.lastChild)];return{same:releaseCaptionNodes.every((node,i)=>node===nodes[i]),text:nodes.map(n=>n.textContent),expected:releaseCaptionText,mutations:releaseCaptionMutations.filter(record=>releaseCaptionNodes.includes(record.target)||releaseCaptionNodes.some(node=>node.parentElement===record.target)).length}})()`);
          assert.ok(retained.same,`${theme}/${tag}/${panel} ordinary brush updates retain caption Text nodes`);assert.deepEqual(retained.text,retained.expected);assert.equal(retained.mutations,0,`${theme}/${tag}/${panel} unchanged captions are not rewritten`);
          await capture(`${theme}-configure-${panel}-${tag}`);
        }
        await send({type:'customize',action:{type:'close_expanded'}});await language('en');
      }
    });
    await check(`${theme}-configure-workspace-replacement`,async()=>{
      const before=await evaluate('layerApp.app.workspace_persistence()'),name=`Release lifecycle ${theme}`;
      const bounds=()=>evaluate(`layerApp.app.layout(innerWidth,innerHeight).groups.map(({id,panels,bounds})=>({id,panels,bounds}))`),geometryBefore=await bounds();
      await send({type:'customize',action:{type:'new_toolbar',group:null}});await wait('!!document.querySelector("#tool-picker[open]")');
      await evaluate(`(()=>{const name=document.querySelector('#toolbar-name');name.value=${JSON.stringify(name)};name.dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('#tool-picker .tool-choice input').click();document.querySelector('#confirm-tools').click()})()`);await settle();
      const panel=await evaluate(`layerApp.state().workspace.layout.panels.find(panel=>panel.content.kind==='toolbar'&&panel.content.name===${JSON.stringify(name)})?.id`);assert.ok(panel,'new custom toolbar exists');
      await send({type:'customize',action:{type:'show_all_controls',panel}});await wait(`layerApp.state().customization.expanded===${JSON.stringify(panel)}&&!!document.querySelector('.panel-configuration')`);
      assert.equal(await evaluate('document.querySelector("#status").textContent'),'','no dispatch errors before workspace replacement');
      const immediate=await evaluate(`(()=>{layerApp.dispatch({type:'restore_workspace',workspace:${JSON.stringify(before)}});return document.querySelector('#status').textContent})()`);assert.equal(immediate,'','workspace replacement does not catch a missing Configure model error');await settle();
      await wait(`layerApp.state().customization.expanded==null&&!document.querySelector('.panel-configuration,.expanded-panel')`);
      assert.ok(await evaluate(`!layerApp.state().workspace.layout.panels.some(p=>p.id===${JSON.stringify(panel)})`),'workspace replacement removes the custom toolbar');
      assert.deepEqual(await bounds(),geometryBefore,'workspace replacement restores the other dock groups and bounds');
      const groups=await evaluate(`[...document.querySelectorAll('.dock-group')].filter(n=>n.checkVisibility()).map(n=>{const r=n.getBoundingClientRect();return{width:r.width,height:r.height}})`);assert.ok(groups.length);assert.ok(groups.every(r=>Number.isFinite(r.width)&&Number.isFinite(r.height)&&r.width>0&&r.height>0),'remaining dock groups retain usable geometry');
      await send({type:'customize',action:{type:'show_all_controls',panel:'sizes'}});await wait(`layerApp.state().customization.expanded==='sizes'&&!!document.querySelector('.panel-configuration h3')`);assert.equal(await evaluate('document.querySelector("#status").textContent'),'','builtin Configure reopens without a stale custom toolbar');await capture(`${theme}-configure-after-workspace-replacement`);await send({type:'customize',action:{type:'close_expanded'}});
    });
    await check(`${theme}-toolbar-all-languages`,async()=>{
      await send({type:'customize',action:{type:'new_toolbar',group:null}});await wait('!!document.querySelector("#tool-picker[open]")');
      await evaluate(`document.querySelector('#tool-picker .tool-choice input').click();window.releaseToolbarName=document.querySelector('#toolbar-name');releaseToolbarName.value='Literal 🎨';releaseToolbarName.dispatchEvent(new Event('input',{bubbles:true}));`);await settle();
      for(const tag of tags) {
        await language(tag);
        const state=await evaluate(`(()=>{const root=document.querySelector('#tool-picker'),head=root.querySelector('header'),title=head.querySelector('h2'),nodes=[...head.children],r=title.getBoundingClientRect();return{title:title.textContent,cancel:head.firstElementChild.textContent,confirm:head.lastElementChild.textContent,count:root.querySelector('.tool-count').textContent,model:JSON.parse(JSON.stringify(layerApp.app.tool_picker(),(_,v)=>typeof v==='bigint'?Number(v):v)),name:document.querySelector('#toolbar-name').value,same:releaseToolbarName===document.querySelector('#toolbar-name'),rects:nodes.map(n=>{const r=n.getBoundingClientRect();return{x:r.x,right:r.right,y:r.y,bottom:r.bottom}}),overflow:title.scrollWidth-title.clientWidth}})()`);
        assert.equal(state.title,copy[tag]['workspace-picker-new-toolbar-title']);assert.equal(state.cancel,copy[tag]['common-cancel']);assert.equal(state.confirm,copy[tag]['workspace-picker-create-toolbar']);assert.equal(state.count,state.model.selected_label);assert.equal(state.model.selected_count,1);assert.ok(state.same);assert.equal(state.name,'Literal 🎨');assert.ok(state.rects.every((a,i)=>state.rects.slice(i+1).every(b=>a.right<=b.x+.5||b.right<=a.x+.5||a.bottom<=b.y+.5||b.bottom<=a.y+.5))&&state.overflow<=1,`${tag} toolbar header does not overlap: ${JSON.stringify(state.rects)}`);
        await fullButtons(`${theme}-${tag}-toolbar`,'#tool-picker > header');await capture(`${theme}-toolbar-${tag}`);
      }
    });
    await send({type:'customize',action:{type:'cancel_tools'}});await language('en');
    await check(`${theme}-shortcut-live`,async()=>{
      await send({type:'invoke',command:'settings'});await send({type:'preferences',action:{type:'page',page:'shortcuts'}});await send({type:'preferences',action:{type:'edit_shortcut',id:'tools.paint'}});await wait('!!document.querySelector("#shortcut-editor[open]")');
      for(const tag of ['de','ja']) {await language(tag);const state=await evaluate(`({text:document.querySelector('#shortcut-editor').textContent,keys:layerApp.app.preferences().shortcut_editor.defaults})`);assert.ok(clean(state.text).includes(copy[tag]['settings-shortcut-default'].replace('{ $keys }',state.keys.join(' / '))),`${tag} shortcut Default summary localized`);await capture(`${theme}-shortcut-${tag}`);}
    });
    await escape();await escape();await language('en');
    await check(`${theme}-new-drawing-live`,async()=>{
      await send({type:'invoke',command:'new_document'});await wait('!!document.querySelector("#new-drawing-dialog[open]")');await evaluate(`window.releaseDrawingName=document.querySelector('#new-drawing-name');window.releaseEnglishUntitled=releaseDrawingName.placeholder;`);
      for(const tag of ['de','ja']) {await language(tag);assert.ok(await evaluate(`releaseDrawingName===document.querySelector('#new-drawing-name')&&releaseDrawingName.placeholder!==releaseEnglishUntitled&&releaseDrawingName.value===''`),`${tag} empty Untitled placeholder follows publication`);await fullSelects(`${theme}-${tag}-new-drawing`,'#new-drawing-dialog');await capture(`${theme}-new-drawing-${tag}`);}
    });
    await escape();await language('en');
    await check(`${theme}-crop-properties-export-workspace`,async()=>{
      await send({type:'invoke',command:'crop'});await send({type:'customize',action:{type:'set_panel_visible',panel:'tool_settings',visible:true}});
      for(const tag of ['de','ja']) {await language(tag);await fullSelects(`${theme}-${tag}-crop`,'[data-control=tool_settings]');await capture(`${theme}-crop-${tag}`);}
      await send({type:'invoke',command:'pen'});await send({type:'customize',action:{type:'set_panel_visible',panel:'properties',visible:true}});
      for(const tag of ['de','ja']) {await language(tag);await fullSelects(`${theme}-${tag}-properties`,'[data-control=properties]');const labels=await evaluate(`[...document.querySelectorAll('[data-control=properties] .property-row > span:first-child,.properties-layer-type')].filter(n=>n.checkVisibility()).map(n=>{const range=document.createRange();range.selectNodeContents(n);const r=n.getBoundingClientRect(),text=range.getBoundingClientRect();return{text:n.textContent,available:r.width,natural:text.width,height:r.height,textHeight:text.height}})`);for(const label of labels)assert.ok(label.natural<=label.available+.6,`${tag} Properties label fits: ${JSON.stringify(label)}`);await capture(`${theme}-properties-${tag}`);}
      await send({type:'invoke',command:'export_document'});await wait('!!document.querySelector(".document-dialog[open]")');
      for(const tag of ['de','ja']) {await language(tag);await fullButtons(`${theme}-${tag}-export`,'.document-dialog[open] footer');await capture(`${theme}-export-${tag}`);}
      await escape();await language('en');await evaluate(`layerApp.app.workspace_input(JSON.stringify({type:'open',page:'workspaces'}));null`);await wait('!!document.querySelector(".workspace-manager[open]")');
      for(const tag of ['en','de','ja']) {await language(tag);await fullButtons(`${theme}-${tag}-workspaces`,'.workspace-manager[open] footer');await capture(`${theme}-workspaces-${tag}`);}
    });
    await escape();await escape();await language('en');
  }
  await send({type:'restore_workspace',workspace:original});await language('en');
  await writeFile(`${directory}/layout.local.json`,JSON.stringify({tags,measurements,failures},null,2));
  assert.deepEqual(failures,[]);
}
