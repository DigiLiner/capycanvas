import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {createDrawing} from './new-drawing-fixture.mjs';

export async function checkTouchScrub({call,evaluate,settle}) {
  const wait=expression=>evaluate(`new Promise((resolve,reject)=>{const end=performance.now()+25000;function check(){if(${expression})resolve();else if(performance.now()>end)reject(Error(${JSON.stringify(expression)}));else setTimeout(check,20)}check()})`);
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();};
  const invoke=command=>send({type:'invoke',command});
  const state=()=>evaluate('JSON.parse(JSON.stringify(layerApp.state(),(_,v)=>typeof v==="bigint"?Number(v):v))');
  const camera=async()=>{const c=(await state()).camera;return{translation:c.translation,zoom:c.zoom,rotation:c.rotation};};
  await invoke('new_document');
  await evaluate(createDrawing({width:1000,height:700}));
  await wait('layerApp.app.brush_ready() && !layerApp.documents.busy()');
  await invoke('fit_canvas');
  await invoke('pen');
  await send({type:'select_brush',id:1});
  await send({type:'set_brush_size',value:40});
  const center=await evaluate('(()=>{const c=layerApp.app.camera(),r=layerApp.canvas.getBoundingClientRect(),a=c.work_area;return{x:r.x+(a[0]+a[2]/2)*r.width/c.viewport[0],y:r.y+(a[1]+a[3]/2)*r.height/c.viewport[1]}})()');
  const stroke=async(from,to)=>{
    await wait('layerApp.app.brush_ready()');
    const revision=await evaluate('String(layerApp.state().layers.find(l=>l.editing).paint_revision)');
    await call('Input.dispatchMouseEvent',{type:'mousePressed',...from,button:'left',buttons:1,clickCount:1,pointerType:'pen',force:.8});
    for(let step=1;step<=8;step++){
      await call('Input.dispatchMouseEvent',{type:'mouseMoved',x:from.x+(to.x-from.x)*step/8,y:from.y+(to.y-from.y)*step/8,button:'left',buttons:1,pointerType:'pen',force:.8});
      await settle();
    }
    await call('Input.dispatchMouseEvent',{type:'mouseReleased',...to,button:'left',buttons:0,pointerType:'pen',force:0});
    await wait(`String(layerApp.state().layers.find(l=>l.editing).paint_revision)!==${JSON.stringify(revision)}`);
    await call('Input.dispatchMouseEvent',{type:'mouseMoved',x:10,y:10,buttons:0,pointerType:'mouse'});
    await settle();
  };
  const shots=new Map();
  const pixels=async()=>{
    await wait('!layerApp.app.canvas_work_pending()');
    await settle();
    await evaluate('layerApp.canvas.getContext("webgpu").getConfiguration().device.queue.onSubmittedWorkDone()');
    await settle();
    const shot=await call('Page.captureScreenshot',{format:'png',clip:{x:center.x-120,y:center.y-120,width:240,height:240,scale:1}});
    const hash=await evaluate(`(async()=>{const image=new Image();image.src=${JSON.stringify(`data:image/png;base64,${shot.data}`)};await image.decode();const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;const context=canvas.getContext('2d');context.drawImage(image,0,0);return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',context.getImageData(0,0,image.width,image.height).data))).join(',')})()`);
    shots.set(hash,shot.data);
    return hash;
  };
  const expectPixels=async(expected,label)=>{
    let actual;
    for(const deadline=Date.now()+15000;Date.now()<deadline;){
      actual=await pixels();
      if(actual===expected)return;
      await new Promise(resolve=>setTimeout(resolve,50));
    }
    const directory=process.env.LAYER_TEST_ARTIFACTS??'artifacts/three-finger-scrub/web';
    await mkdir(directory,{recursive:true});
    await writeFile(`${directory}/expected.png`,Buffer.from(shots.get(expected),'base64'));
    await writeFile(`${directory}/actual.png`,Buffer.from(shots.get(actual),'base64'));
    await writeFile(`${directory}/state.json`,JSON.stringify(await state(),null,2));
    assert.equal(actual,expected,label);
  };
  await send({type:'set_color',rgba:[.1,.3,.8,1]});
  await stroke({x:center.x-90,y:center.y},{x:center.x+90,y:center.y});
  const underneath=await pixels();
  const lower=(await state()).layers.find(layer=>layer.editing);
  await invoke('add_layer');
  await send({type:'set_color',rgba:[.9,.2,.1,1]});
  await stroke({x:center.x,y:center.y-90},{x:center.x,y:center.y+90});
  const painted=await pixels();
  assert.notEqual(painted,underneath,'the active layer adds visible paint');
  let id=900;
  for(const theme of ['light','dark']){
    await send({type:'set_theme',theme});
    for(const axis of [[1,0],[0,1],[.6,.8]]){
      const before=await camera();
      const points=[-35,0,35].map(offset=>({id:++id,x:center.x+offset,y:center.y+offset}));
      const fingers=(type,distance)=>call('Input.dispatchTouchEvent',{type,touchPoints:type==='touchEnd'?[]:points.map(point=>({...point,x:point.x+axis[0]*distance,y:point.y+axis[1]*distance}))});
      await fingers('touchStart',0);
      for(const distance of [90,-90,90]){await fingers('touchMove',distance);await settle();}
      await expectPixels(underneath,`${theme}/${axis}: scrub clears the active layer before lift`);
      const cleared=(await state()).layers.find(layer=>layer.editing).paint_revision;
      for(const distance of [-90,90]){await fingers('touchMove',distance);await settle();}
      await fingers('touchEnd',0);await settle();
      assert.equal((await state()).layers.find(layer=>layer.editing).paint_revision,cleared,'a held scrub clears once');
      assert.equal((await state()).layers.find(layer=>layer.id===lower.id).paint_revision,lower.paint_revision,'the layer underneath remains unchanged');
      assert.deepEqual(await camera(),before,'scrub preserves the camera');
      await invoke('undo');
      await expectPixels(painted,'one Undo restores exact presented paint');
      await invoke('redo');
      await expectPixels(underneath,'Redo clears the layer again');
      await invoke('undo');
    }
  }
  console.log('PASS: three-finger horizontal, vertical and diagonal scrubs clear only the active layer once, preserve the camera, and Undo/Redo exact pixels in both themes');
}
