import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {scopesFixture} from './scopes-fixture.mjs';

export async function checkTransformFootprint({call,evaluate,settle}) {
  const directory=process.env.LAYER_TEST_ARTIFACTS??'artifacts/transform-footprint/web';
  await mkdir(directory,{recursive:true});
  const fixture=await scopesFixture({call,evaluate,settle});
  const {send,invoke,json,checkpoint,documentPoint}=fixture;
  const reports=[],baseline=process.env.LAYER_TRANSFORM_BASELINE==='1';
  const poll=async condition=>{const end=Date.now()+30000;while(Date.now()<end){if(await evaluate(condition))return;await settle();await new Promise(r=>setTimeout(r,50))}throw Error(`Transform did not settle: ${condition}; ${await evaluate("JSON.stringify({error:layerApp.state().host_error,notice:layerApp.state().notice,file:layerApp.state().document_file,tool:layerApp.state().layer_tools.tool},(_,v)=>typeof v==='bigint'?String(v):v)")}`)};
  const pointer=(type,p)=>call('Input.dispatchMouseEvent',{type,...p,button:'left',buttons:type==='mouseReleased'?0:1,clickCount:1,pointerType:'pen',force:type==='mouseReleased'?0:.7});
  const drag=async(from,to,steps=8,beforeFinal)=>{await pointer('mousePressed',await documentPoint(from));for(let i=1;i<=steps;i++){await pointer('mouseMoved',await documentPoint(from.map((v,k)=>v+(to[k]-v)*i/steps)));await settle();if(i===steps-1)await beforeFinal?.()}await pointer('mouseReleased',await documentPoint(to));await settle()};
  const press=async command=>{
    const selector=`.canvas-action-bar [data-command="${command}"]`;
    await poll(`(n=>n&&!n.closest('.canvas-action-bar-item,.canvas-action-bar-completion')?.hidden&&!n.closest('.canvas-action-bar.suppressed'))(document.querySelector('${selector}'))`);
    await fixture.click(selector);
  };
  const corners=async()=>{
    const result=[];
    for(const index of [0,2,8,6]){
      await evaluate(`layerApp.dispatch(layerApp.state().tool_extra.find(c=>c.Choice?.id==='transform-reference').Choice.items[${index}].action);null`);
      await settle();result.push(await evaluate("['transform_x','transform_y'].map(id=>layerApp.state().tool_settings.find(s=>s.id===id).value)"));
    }
    await settle();return result;
  };
  const snapshot=()=>json(`({error:layerApp.state().host_error,notice:layerApp.state().notice,tool:layerApp.state().layer_tools.tool,canvas_bar:layerApp.state().canvas_bar,camera:layerApp.app.camera(),gpu:layerApp.app.gpu_diagnostics(),file:layerApp.state().document_file})`);
  const pixels=async points=>{await fixture.idle();const screen=await Promise.all(points.map(documentPoint)),{data}=await call('Page.captureScreenshot',{format:'png'});return evaluate(`(async()=>{const image=new Image();image.src='data:image/png;base64,'+${JSON.stringify(data)};await image.decode();const c=new OffscreenCanvas(image.width,image.height),x=c.getContext('2d',{willReadFrequently:true});x.drawImage(image,0,0);return ${JSON.stringify(screen)}.map(p=>Array.from(x.getImageData(Math.round(p.x),Math.round(p.y),1,1).data))})()`)};
  try {
    await call('Emulation.setDeviceMetricsOverride',{width:3000,height:2600,deviceScaleFactor:1,mobile:false});
    await evaluate(`(async()=>{const c=new OffscreenCanvas(2048,2048),x=c.getContext('2d'),image=x.createImageData(2048,2048);let seed=0x13579bdf;for(let i=0;i<2048*2048;i++){seed=(Math.imul(seed,1664525)+1013904223)>>>0;image.data.set([32+(seed&191),32+((seed>>>8)&191),32+((seed>>>16)&191),255],i*4)}x.putImageData(image,0,0);window.footprintPhoto=new File([await c.convertToBlob({type:'image/png'})],'dense-transform-2048.png',{type:'image/png'})})()`);
    for(const theme of process.env.LAYER_TEST_THEME?[process.env.LAYER_TEST_THEME]:['light','dark'])for(const filter of process.env.LAYER_TRANSFORM_FILTER?[process.env.LAYER_TRANSFORM_FILTER]:['bilinear','bicubic']){
      const report={theme,filter,requested_quad:[[1022.9555,1022.9555],[2048,0],[2048,2048],[0,2048]]};reports.push(report);
      await send({type:'set_theme',theme});
      await evaluate("window.showOpenFilePicker=async()=>[{name:footprintPhoto.name,getFile:async()=>footprintPhoto}]");
      const epoch=await evaluate('String(layerApp.state().document_file.epoch)');
      await invoke('open_document');await poll(`String(layerApp.state().document_file.epoch)!==${JSON.stringify(epoch)}`);await fixture.idle();
      await invoke('actual_pixels');
      const original=await checkpoint();report.original=original;
      report.original_pixels=await pixels([[100,100],[1540,1230]]);
      assert.ok(report.original_pixels.every(p=>p.slice(0,3).every(v=>v>=25&&v<=230)&&p[3]===255),'Actual hardware canvas displays the dense photo before the transform');
      await invoke('scale_rotate');await poll("layerApp.state().canvas_bar?.context.kind==='transform'");
      await invoke('transform_distort');await invoke(`transform_${filter}`);
      if(await evaluate("layerApp.state().commands.find(c=>c.id==='transform_snapping').selected"))await invoke('transform_snapping');
      report.corner_controls=await json('layerApp.state().tool_extra');report.initial_quad=await corners();
      assert.deepEqual(report.initial_quad,[[0,0],[2048,0],[2048,2048],[0,2048]],'Dense photo starts at exact source corners');
      report.camera=await json('layerApp.app.camera()');
      report.target_screen=await documentPoint([1022.9555,1022.9555]);
      console.log('Near-line Distort input',JSON.stringify({theme,filter,quad:report.requested_quad,camera:report.camera,screen:report.target_screen}));
      await drag([0,0],[1022.9555,1022.9555],8,async()=>{report.last_valid_quad=await corners()});
      report.accepted_quad=await corners();report.after_collapse=await snapshot();
      await new Promise(r=>setTimeout(r,1500));await settle();report.after_render=await snapshot();
      await writeFile(`${directory}/${theme}-${filter}-collapse.png`,Buffer.from((await call('Page.captureScreenshot',{format:'png'})).data,'base64'));
      console.log('Near-line Distort result',JSON.stringify({theme,filter,quad:report.accepted_quad,error:report.after_render.error,gpu:report.after_render.gpu}));
      if(baseline)continue;
      assert.equal(report.after_render.error,null,'Near-line drag leaves the real renderer usable');
      assert.equal(report.after_render.tool,'transform','Near-line drag retains an editable transform');
      assert.deepEqual(report.accepted_quad,report.last_valid_quad,'Rejected near-line geometry retains all four last-valid corner coordinates');
      await drag(report.accepted_quad[0],[512,512]);
      report.backed_out_quad=await corners();
      assert.ok(report.backed_out_quad[0].every(v=>Math.abs(v-512)<.01),'A real pointer drag backs out to the precise valid corner');
      await press('apply_transform');await poll("layerApp.state().layer_tools.tool!=='transform'");await fixture.idle();
      const applied=await checkpoint();assert.notDeepEqual(applied.authored,original.authored,'A backed-out valid Distort publishes a changed paint root');
      report.applied_pixels=await pixels([[100,100],[1540,1230]]);assert.notDeepEqual(report.applied_pixels,report.original_pixels,'Applied Distort changes actual canvas pixels');
      await invoke('undo');await fixture.idle();assert.deepEqual(await checkpoint(),original,'One Undo restores exact source, authored and raster identities');
      assert.deepEqual(await pixels([[100,100],[1540,1230]]),report.original_pixels,'Undo restores actual dense-source canvas pixels');
      await invoke('redo');await fixture.idle();assert.deepEqual(await checkpoint(),applied,'Redo restores the exact transformed document');
      await invoke('pencil');await send({type:'set_color',rgba:[1,0,0,1]});await send({type:'set_brush_size',value:24});
      await drag([1500,1200],[1580,1260]);await fixture.idle();
      const painted=await checkpoint();assert.notDeepEqual(painted.rasters,applied.rasters,'The next physical paint stroke publishes raster pixels');
      report.painted_pixels=await pixels([[100,100],[1540,1230]]);assert.notDeepEqual(report.painted_pixels,report.applied_pixels,'The next stroke changes actual canvas pixels');
      await invoke('undo');await fixture.idle();assert.deepEqual(await checkpoint(),applied,'Painting has its own exact Undo');
      await invoke('redo');await fixture.idle();assert.deepEqual(await checkpoint(),painted,'Painting Redo restores the exact raster');
      await fixture.reopen();assert.deepEqual(await checkpoint(),painted,'Native save/reopen retains transformed artwork and subsequent paint');
      await invoke('actual_pixels');assert.deepEqual(await pixels([[100,100],[1540,1230]]),report.painted_pixels,'Save/reopen restores actual transformed and painted canvas pixels');
      report.completed=true;console.log(`PASS near-line Distort ${theme}/${filter}: backout, Apply, paint, exact Undo/Redo and save/reopen`);
    }
  } finally {
    await writeFile(`${directory}/transform-footprint.json`,JSON.stringify({baseline,reports},null,2));
    await fixture.dispose();await evaluate('delete window.footprintPhoto');
  }
}
