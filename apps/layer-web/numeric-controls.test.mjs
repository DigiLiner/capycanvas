import assert from 'node:assert/strict';
import {mkdir,writeFile,rename} from 'node:fs/promises';
import {existsSync} from 'node:fs';

export async function checkNumericControls({call,evaluate,settle}) {
  const native=process.argv.includes('--native-input'),inputDir=process.env.LAYER_NATIVE_INPUT_DIR;
  const output=process.env.LAYER_TEST_ARTIFACTS||'artifacts/platform-number-controls/web';
  await mkdir(output,{recursive:true});
  let nativeStep=0;
  const performNative=async events=>{
    const step=nativeStep++,path=`${inputDir}/step-${step}.json`;
    await writeFile(`${path}.tmp`,JSON.stringify(events));await rename(`${path}.tmp`,path);
    const deadline=Date.now()+15000;
    while(!existsSync(`${inputDir}/done-${step}`)){assert.ok(Date.now()<deadline,'Compositor numeric input timed out');await new Promise(r=>setTimeout(r,2));}
  };
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();};
  const wait=async()=>{await settle();await evaluate('new Promise(r=>setTimeout(r,80))');};
  const saved=await evaluate('layerApp.state().workspace'),fixture=structuredClone(saved);
  const tabs=(id,panels)=>({kind:'tabs',id,panels,active:panels[0],tab_style:'icon'});
  Object.assign(fixture.layout,{bands:[{id:40,edge:'left',extent:310,root:tabs(41,['sizes'])},{id:42,edge:'right',extent:310,root:tabs(43,['tool_settings'])},{id:44,edge:'top',extent:36,root:tabs(45,['toolbar'])}],floating:[],collapsed:[],column_scroll:[],fit_tab_groups:[],fit_height_groups:[],column_stacks:[],next_id:Math.max(46,fixture.layout.next_id)});
  fixture.zen_mode=false;
  const size='.dock-group[data-group="43"] [data-tool-setting="size"]';
  const point=selector=>evaluate(`(()=>{const n=document.querySelector(${JSON.stringify(selector)});n.scrollIntoView({block:'nearest'});const r=n.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2}})()`);
  const input=async(device,type,p,fine=false)=>{
    if(native&&device!=='pen')return performNative([device==='touch'?{touch:type,point:[p.x,p.y]}:type==='up'?{down:false}:{point:[p.x,p.y],...(type==='down'?{down:true}:{})}]);
    if(device==='touch')return call('Input.dispatchTouchEvent',{type:{down:'touchStart',move:'touchMove',up:'touchEnd'}[type],touchPoints:type==='up'?[]:[{...p,id:1}],modifiers:fine?8:0});
    return call('Input.dispatchMouseEvent',{type:{down:'mousePressed',move:'mouseMoved',up:'mouseReleased'}[type],...p,button:'left',buttons:type==='up'?0:1,clickCount:1,pointerType:device,modifiers:fine?8:0});
  };
  const key=async name=>{
    for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:name,code:name,windowsVirtualKeyCode:name==='Escape'?27:13});
  };
  const shift=down=>native?performNative([{key:0xffe1,down}]):call('Input.dispatchKeyEvent',{type:down?'keyDown':'keyUp',key:'Shift',code:'ShiftLeft',windowsVirtualKeyCode:16,modifiers:down?8:0});
  const text=selector=>evaluate(`document.querySelector(${JSON.stringify(selector+' .number-value')}).textContent`);
  const close=(actual,expected,label)=>assert.ok(Math.abs(actual-expected)<.001,`${label}: ${actual} != ${expected}`);
  const geometry=selector=>evaluate(`(()=>{const n=document.querySelector(${JSON.stringify(selector)}),box=n.getBoundingClientRect(),slider=n.querySelector('.number-slider'),track=slider.getBoundingClientRect(),v=n.querySelector('.number-value'),value=v.getBoundingClientRect(),s=getComputedStyle(v),r=document.createRange();r.selectNodeContents(v);const glyph=r.getBoundingClientRect();return{box:box.toJSON(),track:track.toJSON(),value:value.toJSON(),glyph:glyph.toJSON(),font:s.fontFamily,paddingLeft:parseFloat(s.paddingLeft),paddingRight:parseFloat(s.paddingRight),overflow:s.overflow,textOverflow:s.textOverflow,trackHeight:[...document.styleSheets].flatMap(sheet=>[...sheet.cssRules]).find(rule=>rule.selectorText==='.number-slider::-webkit-slider-runnable-track')?.style.height,text:v.textContent}})()`);
  const geometryReports=[],motion=[],fonts=[];
  await call('DOM.enable');await call('CSS.enable');
  const platformFonts=async(selector,text)=>{
    const css=await evaluate(`(()=>{const n=document.createElement('span');n.id='numeric-ui-font-reference';n.textContent=${JSON.stringify(text)};n.style.cssText='position:fixed;left:0;top:0;z-index:2147483647;font:inherit;font-variant-numeric:tabular-nums;white-space:pre;pointer-events:none';n.setAttribute('aria-hidden','true');document.documentElement.append(n);return{root:getComputedStyle(document.documentElement).fontFamily,control:getComputedStyle(document.querySelector(${JSON.stringify(selector)})).fontFamily}})()`);
    try {
      assert.equal(css.control,css.root,'numeric controls inherit the configured root UI font');
      assert.ok(css.root.includes('Adwaita Sans')&&css.root.includes('system-ui'),'preferred UI font and system fallback policy');
      await evaluate('document.fonts.ready.then(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))))');
      const {root}=await call('DOM.getDocument');
      const collect=async selector=>{const {nodeId}=await call('DOM.querySelector',{nodeId:root.nodeId,selector});const result=await call('CSS.getPlatformFontsForNode',{nodeId});assert.ok(result.fonts.length&&result.fonts.every(font=>font.glyphCount>0),'actual glyph fonts: '+JSON.stringify(result));return result;};
      const reference=await collect('#numeric-ui-font-reference'),result=await collect(selector);
      const families=fonts=>[...new Set(fonts.map(font=>font.familyName))].sort();
      assert.deepEqual(families(result.fonts),families(reference.fonts),'numeric glyphs match the independently inherited UI font');
      return{...result,reference,fontFamily:css.root,text};
    } finally {await evaluate("document.querySelector('#numeric-ui-font-reference')?.remove()");}
  };
  if(native)await writeFile(`${inputDir}/ready`,'ready');
  try {
    if(native)await performNative([{key:0xffe1,down:true},{key:0xffe1,down:false},{wait_ms:100}]);
    await send({type:'restore_workspace',workspace:fixture});
    for(const theme of ['light','dark']) {
      await send({type:'set_theme',theme});
      const measured=[];
      for(const value of [999,1000,1200.5,2048]) {
        await send({type:'set_brush_size',value});await wait();const g=await geometry(size);measured.push(g);fonts.push({theme,value,role:'readout',...await platformFonts(size+' .number-value',`${value} px`)});
        assert.equal(g.text,`${value} px`,'complete number and unit');close(g.box.height,36,'panel height');close(g.track.height,16,'slider hit height');assert.equal(g.trackHeight,'4px','visible track height');close(g.track.x-g.box.x,36,'track left');close(g.box.right-g.track.right,76,'track right');close(g.value.bottom,g.box.bottom,'readout bottom');close(g.value.right,g.box.right,'readout right');
        close(g.glyph.right,g.value.right-g.paddingRight,'complete suffix right anchor');assert.ok(g.glyph.left>=g.value.left+g.paddingLeft-.5,'whole readout fits');assert.notEqual(g.textOverflow,'ellipsis');assert.ok(g.font.includes('Adwaita Sans'));
        if(Number.isInteger(value))assert.ok(g.glyph.left>=g.track.right+6,'integer readout clears slider');
        const p=await point(size+' .number-value');await input('mouse','down',p);await input('mouse','up',p);await wait();
        const entry=await evaluate(`document.querySelector('${size} .number-entry').getBoundingClientRect().toJSON()`);close(entry.width,68,'entry width');close(entry.height,24,'entry height');close(entry.bottom,g.box.bottom,'entry bottom');close(entry.right,g.box.right,'entry right');fonts.push({theme,value,role:'entry',...await platformFonts(size+' .number-entry',String(value))});
        close(entry.left-g.track.right,8,'entry slider gap');const editingTrack=await evaluate(`document.querySelector('${size} .number-slider').getBoundingClientRect().toJSON()`);close(editingTrack.left,g.track.left,'stable slider left');close(editingTrack.right,g.track.right,'stable slider right');await key('Escape');await wait();
      }
      close(measured[0].glyph.right,measured[1].glyph.right,'999 and 1000 suffix anchor');geometryReports.push({theme,measured});
      for(const device of ['mouse','touch','pen']) {
        for(const [origin,pixels,cancel] of [[1200,8,false],[1200,-8,false],[999,8,false],[1000,-8,false],[1200.5,16,true]]) {
          await send({type:'set_brush_size',value:origin});const p=await point(size+' .number-value'),q={x:p.x,y:p.y-pixels};
          await input(device,'down',p);await input(device,'move',q);await wait();
          const value=await evaluate('layerApp.state().brush.diameter'),readout=await text(size);
          if(value>=1000){assert.equal(value,Math.round(value),`${device}: large scrub integer`);assert.match(readout,/^\d+ px$/);}
          if(origin===999)assert.ok(value>=1000,'scrub crosses upper threshold');if(origin===1000)assert.ok(value<1000,'scrub crosses lower threshold');
          if(cancel){await key('Escape');await wait();close(await evaluate('layerApp.state().brush.diameter'),origin,'Escape restores origin');}
          await input(device,'up',q);await wait();
          close(await evaluate('layerApp.state().brush.diameter'),cancel?origin:value,'release retains result');
        }
      }
      await send({type:'set_brush_size',value:1200});const p=await point(size+' .number-value');await input('mouse','down',p);await input('mouse','up',p);await call('Input.insertText',{text:'1200.5'});await key('Enter');await wait();close(await evaluate('layerApp.state().brush.diameter'),1200.5,'manual fractional value');assert.equal(await text(size),'1200.5 px');
      const percent=await evaluate(`(()=>{const f=layerApp.state().tool_settings.find(f=>f.id==='opacity');return f?{id:f.id,value:f.value}:null})()`);assert.ok(percent,'brush opacity numeric field');
      const pct='.dock-group[data-group="43"] [data-tool-setting="opacity"]';
      for(const device of ['mouse','touch','pen']) {
        for(const [origin,pixels,expected,readout] of [[120,8,122,'122 px'],[12,6,13.5,'13.5 px']]) {
          await send({type:'set_brush_size',value:origin});const p=await point(size+' .number-value'),q={x:p.x,y:p.y-pixels};
          await input(device,'down',p);await input(device,'move',{x:p.x,y:p.y-20});await input(device,'move',q);await wait();
          close(await evaluate('layerApp.state().brush.diameter'),expected,`${device}: pixel scrub value`);
          assert.equal(await text(size),readout,`${device}: pixel readout while held`);
          await input(device,'up',q);await wait();assert.equal(await text(size),readout,'release preserves pixel precision');
        }
        for(const [origin,pixels,fine,expected,readout] of [[.5,8,false,.52,'52 %'],[.02,6,false,.035,'3.5 %'],[.5,20,true,.505,'50.5 %']]) {
          await send({type:'set_tool_setting',id:percent.id,value:origin});const p=await point(pct+' .number-value'),q={x:p.x,y:p.y-pixels};
          if(fine)await shift(true);
          try {
            await input(device,'down',p,fine);await input(device,'move',{x:p.x,y:p.y-20},fine);await input(device,'move',q,fine);await wait();
            close(await evaluate(`layerApp.state().tool_settings.find(f=>f.id==='opacity').value`),expected,`${device}: percentage scrub value`);
            assert.equal(await text(pct),readout,`${device}: percentage readout while held`);
            await input(device,'up',q,fine);await wait();assert.equal(await text(pct),readout,'release preserves percentage precision');
          } finally {if(fine)await shift(false);}
        }
      }
      const shot=await call('Page.captureScreenshot',{format:'png'});await writeFile(`${output}/numeric-${theme}.png`,Buffer.from(shot.data,'base64'));
      if(native) {
        const events=(p,count,legs)=>[{point:[p.x,p.y]},{down:true},...Array.from({length:count},(_,i)=>{const phase=(i+1)*legs/count,leg=Math.floor(phase),fraction=phase-leg;return{point:[p.x,p.y+240*(leg%2?1-fraction:fraction)]}}),{down:false}];
        await send({type:'set_brush_size',value:1200});await performNative(events(await point(size+' .number-value'),250,2));await wait();
        for(let contact=0;contact<3;contact++) {
          await send({type:'set_brush_size',value:1200});const p=await point(size+' .number-value');await evaluate(`window.numericMotion={running:true,frames:[],values:[]};(()=>{const number=document.querySelector('${size}'),tick=t=>{const p=numericMotion;if(!p.running)return;p.frames.push(t);p.values.push(number.getValue());requestAnimationFrame(tick)};requestAnimationFrame(tick)})()`);
          await performNative(events(p,1500,16));await wait();const probe=await evaluate('(()=>{numericMotion.running=false;return numericMotion})()');const frames=probe.frames.filter((_,i)=>i&&probe.values[i]!==probe.values[i-1]),gaps=frames.slice(1).map((time,i)=>time-frames[i]).sort((a,b)=>a-b),durationMs=frames.at(-1)-frames[0];assert.ok(frames.length>100&&durationMs>=5000,'sustained numeric motion');await writeFile(`${output}/callbacks-${theme}-${contact}.json`,JSON.stringify(probe));motion.push({theme,contact,input:'Wayland mouse',durationMs,callbacksWithChangedValue:frames.length,callbackHz:(frames.length-1)*1000/durationMs,gapP99Ms:gaps[Math.floor(gaps.length*.99)],qualification:false});
        }
      }
    }
    await writeFile(`${output}/fonts.json`,JSON.stringify(fonts,null,2));await writeFile(`${output}/geometry.json`,JSON.stringify(geometryReports,null,2));await writeFile(`${output}/motion.json`,JSON.stringify(motion,null,2));
    console.log('PASS: numeric panel geometry, full readouts, integer scrubs, fractional typing, cancellation, mouse/touch/pen, light/dark');console.log(JSON.stringify(motion));
  } finally {await send({type:'restore_workspace',workspace:saved});if(native)await writeFile(`${inputDir}/finished`,'finished');}
}
