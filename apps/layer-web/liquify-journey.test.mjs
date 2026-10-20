import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdir,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {createDrawing} from './new-drawing-fixture.mjs';

const summary=values=>{
  const sorted=values.toSorted((a,b)=>a-b);
  const percentile=p=>sorted[Math.floor((sorted.length-1)*p)];
  return sorted.length?{count:sorted.length,p50:percentile(.5),p95:percentile(.95),p99:percentile(.99),max:sorted.at(-1)}:null;
};

export async function checkLiquify({call,evaluate,settle},{benchmark=false}={}) {
  const output=process.env.LAYER_TEST_ARTIFACTS??'artifacts/liquify/web';
  await mkdir(output,{recursive:true});
  const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  const wait=async(expression,timeout=30000)=>{
    const end=Date.now()+timeout;
    while(!await evaluate(expression)) {
      if(Date.now()>end)throw Error(`Timed out waiting for ${expression}`);
      await pause(50);
    }
  };
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();};
  const invoke=command=>send({type:'invoke',command});
  const revision=()=>evaluate('String(layerApp.state().layers.find(l=>l.editing).paint_revision)');
  const screen=(x,y)=>evaluate(`(()=>{const c=layerApp.app.camera(),r=layerApp.canvas.getBoundingClientRect();return{x:r.x+(${x}*c.zoom+c.translation[0])*r.width/c.viewport[0],y:r.y+(${y}*c.zoom+c.translation[1])*r.height/c.viewport[1]}})()`);
  const pointer=async(type,p)=>{
    await call('Input.dispatchMouseEvent',{type,...p,button:'left',buttons:type==='mouseReleased'?0:1,clickCount:1,pointerType:'pen',force:type==='mouseReleased'?0:.65});
    await settle();
  };
  const stroke=async(y=240)=>{
    await wait('layerApp.app.brush_ready()');
    const points=[];
    for(let i=0;i<=8;i++)points.push(await screen(220+i*25,y));
    await pointer('mousePressed',points[0]);
    for(const p of points.slice(1))await pointer('mouseMoved',p);
    await pointer('mouseReleased',points.at(-1));
    await pause(250);
  };
  const capture=async name=>{
    await call('Input.dispatchMouseEvent',{type:'mouseMoved',x:10,y:10,buttons:0,pointerType:'mouse'});
    await wait('layerApp.app.brush_ready()');
    await settle();await pause(250);
    const topLeft=await screen(0,0),bottomRight=await screen(640,480);
    const clip={x:topLeft.x,y:topLeft.y,width:bottomRight.x-topLeft.x,height:bottomRight.y-topLeft.y,scale:1};
    const {data}=await call('Page.captureScreenshot',{format:'png',clip});
    const bytes=Buffer.from(data,'base64');
    if(name)await writeFile(join(output,`${name}.png`),bytes);
    return createHash('sha256').update(bytes).digest('hex');
  };
  const originalTheme=await evaluate('layerApp.state().settings.theme??null');
  try {
    await invoke('new_document');
    await evaluate(createDrawing({width:640,height:480}));
    await wait('layerApp.app.brush_ready()');
    await invoke('fit_canvas');
    await invoke('pencil');
    await send({type:'set_color',rgba:[.05,.1,.7,1]});
    await send({type:'set_brush_size',value:24});
    const blank=await capture();
    for(const y of [120,180,240,300,360])await stroke(y);
    for(const theme of ['light','dark']) {
      await send({type:'set_theme',theme});
      await invoke('liquify');
      const pattern=await capture(`${theme}-pattern`);
      assert.notEqual(pattern,blank,`${theme}: painted fixture is visible`);
      const modes=[['Pinch',37],['Push',12],['Twirl counterclockwise',13],['Twirl clockwise',36],['Expand',38],['Crystals',39],['Edge',63],['Reconstruct',64]];
      let before=pattern;
      for(const [name,id] of modes) {
        await send({type:'select_brush',id});
        await send({type:'set_brush_size',value:180});
        assert.equal(await evaluate('layerApp.state().brush.preset'),id,`${theme}: ${name} is selected`);
        assert.equal(await evaluate('layerApp.state().tool_settings.some(s=>s.id==="strength")'),true,`${theme}: ${name} exposes Strength`);
        const oldRevision=await revision();
        await stroke();
        await wait(`String(layerApp.state().layers.find(l=>l.editing).paint_revision)!==${JSON.stringify(oldRevision)}`);
        const after=await capture();
        assert.notEqual(after,before,`${theme}: ${name} changes visible pixels`);
        await invoke('undo');
        await wait(`String(layerApp.state().layers.find(l=>l.editing).paint_revision)===${JSON.stringify(oldRevision)}`);
        assert.equal(await capture(),before,`${theme}: Undo restores pixels before ${name}`);
        await invoke('redo');
        await wait(`String(layerApp.state().layers.find(l=>l.editing).paint_revision)!==${JSON.stringify(oldRevision)}`);
        assert.equal(await capture(),after,`${theme}: Redo restores ${name}`);
        before=after;
        if(id===37) {
          await invoke('pencil');
          assert.equal(await capture(),after,`${theme}: leaving Liquify keeps its pixels`);
          await invoke('liquify');
        }
      }
      await send({type:'select_brush',id:12});
      await send({type:'set_tool_setting',id:'strength',value:0});
      const beforeZero=await revision();
      await stroke();
      assert.equal(await revision(),beforeZero,`${theme}: zero Strength creates no edit`);
      assert.equal(await capture(),before,`${theme}: zero Strength preserves pixels`);
      await send({type:'reset_tool_setting',id:'strength'});
      assert.equal(await evaluate('layerApp.state().commands.find(c=>c.id==="liquify_adjust").enabled'),true);
      await invoke('liquify_adjust');
      assert.equal(await evaluate('layerApp.state().tool_settings.find(s=>s.id==="liquify_amount")?.value'),1);
      await send({type:'set_tool_setting',id:'liquify_amount',value:.5});
      const half=await capture(`${theme}-amount-half`);
      assert.notEqual(half,before,`${theme}: Amount changes the image`);
      await send({type:'set_tool_setting',id:'liquify_amount',value:0});
      assert.equal(await evaluate('layerApp.state().tool_settings.find(s=>s.id==="liquify_amount")?.value'),0);
      assert.equal(await capture(),pattern,`${theme}: zero Amount shows the starting pattern`);
      await send({type:'reset_tool_setting',id:'liquify_amount'});
      assert.equal(await capture(),before,`${theme}: Reset Amount restores the deformation`);
      await invoke('liquify_reset');
      assert.equal(await capture(`${theme}-reset`),pattern,`${theme}: Reset Liquify restores the pattern`);
      await invoke('undo');
      assert.equal(await capture(),before,`${theme}: Undo restores the deformation after Reset`);
      await invoke('liquify_reset');
      assert.equal(await capture(),pattern);
      console.log(`PASS Liquify ${theme}: modes, visible pixels, Undo/Redo, tool switching, zero Strength, Amount and Reset`);
    }
    if(benchmark) {
      const originalPattern=await capture('amount-bench-original');
      await send({type:'select_brush',id:12});
      await send({type:'set_brush_size',value:300});
      for(const y of [160,200,240,280,320])await stroke(y);
      const full=await capture('amount-bench-full');
      await invoke('liquify_adjust');
      await evaluate(`layerApp.dispatch({type:'customize',action:{type:'set_panel_visible',panel:'stats',visible:true}});
        layerApp.dispatch({type:'move_panel',panel:'stats',viewport:[innerWidth,innerHeight],target:{kind:'float',position:[innerWidth-310,90]}})`);
      await settle();
      await send({type:'tool_setting_contact',id:'liquify_amount',phase:'down'});
      await evaluate(`window.liquifyAmountBench={active:true,frameMs:[],dispatchMs:[],updateTimes:[]};
        liquifyAmountBench.frame=layerApp.app.frame;
        layerApp.app.frame=function(...args){const start=performance.now();try{return liquifyAmountBench.frame.apply(this,args)}finally{if(liquifyAmountBench.active)liquifyAmountBench.frameMs.push(performance.now()-start)}}`);
      try {
        await evaluate(`new Promise((resolve,reject)=>{
          const bench=liquifyAmountBench,start=performance.now();
          function tick(now){
            if(now-start>=3000){bench.active=false;resolve(true);return}
            const value=0.15+0.7*(0.5+0.5*Math.sin((now-start)/190));
            const dispatched=performance.now();
            try{layerApp.dispatch({type:'set_tool_setting',id:'liquify_amount',value})}catch(error){bench.active=false;reject(error);return}
            bench.dispatchMs.push(performance.now()-dispatched);bench.updateTimes.push(now);
            requestAnimationFrame(tick);
          }
          requestAnimationFrame(tick);
        })`);
        const moving=await evaluate(`({frameMs:liquifyAmountBench.frameMs,dispatchMs:liquifyAmountBench.dispatchMs,
          updateTimes:liquifyAmountBench.updateTimes,stats:JSON.parse(JSON.stringify(layerApp.app.renderer_stats(),(_,value)=>typeof value==='bigint'?Number(value):value))})`);
        await send({type:'set_tool_setting',id:'liquify_amount',value:.6});
        await send({type:'tool_setting_contact',id:'liquify_amount',phase:'up'});
        await evaluate("layerApp.dispatch({type:'customize',action:{type:'set_panel_visible',panel:'stats',visible:false}})");
        await wait('!layerApp.app.canvas_work_pending()');
        await evaluate('layerApp.canvas.getContext("webgpu").getConfiguration().device.queue.onSubmittedWorkDone()');
        await settle();
        const final=await capture('amount-bench-final');
        assert.notEqual(final,full,'Amount scrub changes the deformed image');
        const committedRevision=await revision();
        await send({type:'tool_setting_contact',id:'liquify_amount',phase:'down'});
        await send({type:'set_tool_setting',id:'liquify_amount',value:0});
        assert.equal(await revision(),committedRevision,'moving Amount leaves document history untouched');
        await send({type:'tool_setting_contact',id:'liquify_amount',phase:'cancel'});
        await wait('!layerApp.app.canvas_work_pending()');
        assert.equal(await revision(),committedRevision,'cancelling Amount creates no edit');
        assert.equal(await capture(),final,'cancelling Amount restores the committed pixels');
        await send({type:'tool_setting_contact',id:'liquify_amount',phase:'down'});
        await send({type:'set_tool_setting',id:'liquify_amount',value:0});
        await send({type:'tool_setting_contact',id:'liquify_amount',phase:'up'});
        await wait('!layerApp.app.canvas_work_pending()');
        assert.equal(await capture('amount-bench-zero'),originalPattern,'settled zero Amount shows the original pattern');
        await send({type:'tool_setting_contact',id:'liquify_amount',phase:'down'});
        await send({type:'set_tool_setting',id:'liquify_amount',value:1});
        await send({type:'tool_setting_contact',id:'liquify_amount',phase:'up'});
        await wait('!layerApp.app.canvas_work_pending()');
        assert.equal(await capture('amount-bench-one'),full,'settled full Amount restores the complete deformation');
        await send({type:'tool_setting_contact',id:'liquify_amount',phase:'down'});
        await send({type:'set_tool_setting',id:'liquify_amount',value:.6});
        await send({type:'tool_setting_contact',id:'liquify_amount',phase:'up'});
        await wait('!layerApp.app.canvas_work_pending()');
        assert.equal(await capture(),final,'Amount can revisit its exact final value');
        await invoke('undo');
        await wait('!layerApp.app.canvas_work_pending()');
        assert.equal(await capture(),full,'Undo restores the full deformation after scrub');
        await invoke('redo');
        await wait('!layerApp.app.canvas_work_pending()');
        assert.equal(await capture(),final,'Redo restores the final Amount after scrub');
        await invoke('liquify_adjust');
        assert.equal(await evaluate('layerApp.state().tool_settings.find(s=>s.id==="liquify_amount")?.value'),1);
        await send({type:'customize',action:{type:'set_panel_visible',panel:'tool_settings',visible:true}});
        await send({type:'move_panel',panel:'tool_settings',target:{kind:'float',position:[20,20]}});
        await evaluate(`(()=>{const workspace=layerApp.state().workspace,group=workspace.layout.floating.find(group=>group.root.panels?.includes('tool_settings'));if(!group)throw Error('Tool Settings did not float');group.width=300;group.height=Math.min(400,innerHeight-40);layerApp.dispatch({type:'restore_workspace',workspace})})()`);
        await settle();
        const slider=await evaluate(`(()=>{const node=document.querySelector('.tool-settings-control [data-tool-setting="liquify_amount"] .number-slider');if(!node)throw Error('Amount slider missing');node.scrollIntoView({block:'center'});const rect=node.getBoundingClientRect(),x=rect.x+rect.width*.8,y=rect.y+rect.height/2;if(!node.checkVisibility()||!node.contains(document.elementFromPoint(x,y)))throw Error('Amount slider is outside the visible panel');return{x,y,left:rect.x,width:rect.width}})()`);
        const beforePointer=await revision();
        await pointer('mousePressed',{x:slider.x,y:slider.y});
        for(const fraction of [.7,.6,.5,.4])await pointer('mouseMoved',{x:slider.left+slider.width*fraction,y:slider.y});
        assert.equal(await revision(),beforePointer,'dragging the native Amount slider leaves history untouched');
        await pointer('mouseReleased',{x:slider.left+slider.width*.4,y:slider.y});
        await pause(50);
        await wait('!layerApp.app.canvas_work_pending()');
        const pointerValue=await evaluate('layerApp.state().tool_settings.find(s=>s.id==="liquify_amount")?.value');
        assert.ok(pointerValue>.2&&pointerValue<.7,`native Amount slider moved to ${pointerValue}`);
        await send({type:'customize',action:{type:'set_panel_visible',panel:'tool_settings',visible:false}});
        const pointerFinal=await capture('amount-bench-pointer-final');
        assert.notEqual(pointerFinal,final,'native Amount slider changes the image');
        await invoke('undo');
        assert.equal(await capture(),final,'Undo restores the deformation before the native slider gesture');
        await invoke('redo');
        assert.equal(await capture(),pointerFinal,'Redo restores the native slider result');
        const intervals=moving.updateTimes.slice(1).map((value,index)=>value-moving.updateTimes[index]);
        const report={agent:await evaluate('navigator.userAgent'),viewport:await evaluate('[innerWidth,innerHeight,devicePixelRatio]'),duration_ms:3000,
          updates:moving.updateTimes.length,update_interval_ms:summary(intervals),host_frame_ms:summary(moving.frameMs),dispatch_ms:summary(moving.dispatchMs),
          render_cpu_ms:summary(moving.stats.samples??[]),gpu_ms:summary(moving.stats.gpu_samples??[]),resident_bytes:moving.stats.resident_bytes,
          raw:{frame_ms:moving.frameMs,dispatch_ms:moving.dispatchMs,update_times:moving.updateTimes,gpu_ms:moving.stats.gpu_samples??[]}};
        await writeFile(join(output,'liquify-amount-bench.json'),JSON.stringify(report,null,2)+'\n');
        console.log('PASS Liquify Amount scrub',JSON.stringify({updates:report.updates,update_interval_ms:report.update_interval_ms,host_frame_ms:report.host_frame_ms,dispatch_ms:report.dispatch_ms,render_cpu_ms:report.render_cpu_ms,gpu_ms:report.gpu_ms}));
      } finally {
        await evaluate("layerApp.dispatch({type:'tool_setting_contact',id:'liquify_amount',phase:'cancel'});layerApp.app.frame=liquifyAmountBench.frame;delete window.liquifyAmountBench;layerApp.dispatch({type:'customize',action:{type:'set_panel_visible',panel:'stats',visible:false}})");
      }
    }
  } catch(error) {
    await capture('failure');
    console.error('Liquify state',await evaluate('JSON.stringify({host_error:layerApp.state().host_error,tool:layerApp.state().layer_tools.tool,brush:layerApp.state().brush.preset,settings:layerApp.state().tool_settings.map(s=>[s.id,s.value]),status:document.querySelector("#status")?.textContent},(_,v)=>typeof v==="bigint"?String(v):v)'));
    throw error;
  } finally {
    await send({type:'set_theme',theme:originalTheme});
  }
}
