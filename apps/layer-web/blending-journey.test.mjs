import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {createDrawing} from './new-drawing-fixture.mjs';

const image='.header-menu[data-menu="image"]';
const dialog='dialog[open]';

export async function checkBlending({call,evaluate,settle}) {
  const directory=process.env.LAYER_TEST_ARTIFACTS??'artifacts/blending/web';
  await mkdir(directory,{recursive:true});
  const wait=(expression,timeout=25000)=>evaluate(`new Promise((resolve,reject)=>{const end=performance.now()+${timeout};function check(){if(${expression})resolve(true);else if(performance.now()>end)reject(Error(${JSON.stringify(expression)}));else setTimeout(check,30);}check();})`);
  const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();};
  const enabled=command=>`(c=>!c||c.enabled)(layerApp.state().commands.find(c=>c.id===${JSON.stringify(command)}))`;
  const invoke=async command=>{await wait(enabled(command));await send({type:'invoke',command});};
  const selected=command=>`!!layerApp.state().commands.find(c=>c.id===${JSON.stringify(command)})?.selected`;
  const rect=expression=>evaluate(`(()=>{const r=(${expression}).getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height}})()`);
  const middle=async expression=>{const r=await rect(expression);return{x:r.x+r.width/2,y:r.y+r.height/2};};
  let touchId=1400;
  const pointer=(type,p,kind)=>kind==='touch'
    ?call('Input.dispatchTouchEvent',{type:{mousePressed:'touchStart',mouseReleased:'touchEnd'}[type],touchPoints:type==='mouseReleased'?[]:[{id:touchId,x:p.x,y:p.y}]})
    :call('Input.dispatchMouseEvent',{type,...p,button:'left',buttons:type==='mouseReleased'?0:1,clickCount:1,pointerType:kind,force:type==='mouseReleased'?0:.6});
  const tap=async(p,kind)=>{touchId++;await pointer('mousePressed',p,kind);await pointer('mouseReleased',p,kind);await settle();await pause(60);};
  const menuRow=label=>`[...document.querySelectorAll('.header-menu[open] .popover button')].find(b=>b.querySelector('.menu-label')?.textContent===${JSON.stringify(label)})`;
  const calm=()=>evaluate('new Promise(resolve=>{let last=performance.now(),steady=0;const frame=t=>{steady=t-last<40?steady+1:0;last=t;if(steady>=3)resolve();else requestAnimationFrame(frame);};requestAnimationFrame(frame);})');
  const choose=async(label,kind)=>{
    await calm();
    await tap(await middle(`document.querySelector('${image} > summary')`),kind);
    await wait(`document.querySelector('${image}').open`);
    for(const row of ['Blending',label]) {
      await wait(`!!${menuRow(row)}&&!${menuRow(row)}.disabled`);
      await tap(await middle(menuRow(row)),kind);
    }
    await wait(`!document.querySelector('${image}').open`);
  };
  const capture=async(file,expression)=>{
    const r=await rect(expression);
    const clip={x:Math.max(0,r.x-8),y:Math.max(0,r.y-8),width:r.width+16,height:r.height+16,scale:1};
    await writeFile(`${directory}/${file}.png`,Buffer.from((await call('Page.captureScreenshot',{format:'png',clip})).data,'base64'));
  };
  const drawing='#new-drawing-dialog[open]',opened=`!!document.querySelector('${drawing}')&&!!layerApp.state().new_drawing`;
  const blending=`document.querySelector('${drawing} #new-drawing-blending')`;
  const setup=()=>evaluate('layerApp.state().new_drawing.setup');
  const choice=async(id,key,value)=>{await evaluate(`(n=>{n.value=String(layerApp.state().new_drawing.${key}.findIndex(c=>c.value===${JSON.stringify(value)}));n.dispatchEvent(new Event('change',{bubbles:true}));})(document.querySelector('${drawing} #new-drawing-${id}'))`);await settle();};
  const cancel=async()=>{await evaluate(`document.querySelector('${drawing} #new-drawing-cancel').click()`);await wait(`!document.querySelector('${drawing}')`);};
  const theme=await evaluate('layerApp.state().settings.theme ?? null');
  await wait(`(()=>{[...document.querySelectorAll('${dialog} button')].find(b=>b.textContent==='Keep for Later')?.click();return !document.querySelector('${dialog}');})()`);
  try {
    assert.equal(await evaluate(selected('blend_perceptual')),true,'a new 8-bit drawing blends perceptually');
    for(const [kind,label,command] of [['mouse','Linear Light Blending','blend_linear'],['touch','Perceptual Blending','blend_perceptual'],['pen','Linear Light Blending','blend_linear']]) {
      await choose(label,kind);
      await wait(selected(command));
    }
    await invoke('undo');
    await wait(selected('blend_perceptual'));
    assert.equal(await evaluate(selected('blend_linear')),false,'one undo step restores Perceptual');

    await invoke('document_properties');
    await wait(`[...document.querySelectorAll('${dialog} h3')].some(h=>h.textContent==='Blending'&&h.nextElementSibling?.textContent==='Perceptual Blending')`);
    await capture('properties',`document.querySelector('${dialog}')`);
    await evaluate(`[...document.querySelectorAll('${dialog} button')].find(b=>b.textContent==='Done').click()`);
    await wait(`!document.querySelector('${dialog}')`);

    for(const name of ['light','dark']) {
      await send({type:'set_theme',theme:name});
      await invoke('new_document');
      await wait(opened);
      assert.equal((await setup()).blend_space,'Perceptual');
      await capture(`new-document-${name}`,`document.querySelector('${drawing}')`);
      await choice('depth','depths','F16');
      assert.deepEqual(await evaluate(`[${blending}.disabled,layerApp.state().new_drawing.setup.blend_space,${blending}.closest('label').title]`),[true,'Linear','Float documents blend in linear light']);
      await capture(`new-document-float-${name}`,`document.querySelector('${drawing}')`);
      await choice('depth','depths','U8');
      assert.deepEqual(await evaluate(`[${blending}.disabled,layerApp.state().new_drawing.setup.blend_space]`),[false,'Perceptual'],'the choice returns with an 8-bit depth');
      await cancel();
    }
    await invoke('new_document');
    await wait(opened);
    await choice('blending','blendings','Linear');
    await evaluate(createDrawing({}));
    await wait(`!document.querySelector('${drawing}')&&${selected('blend_linear')}`);

    const create=async ppi=>{
      await invoke('new_document');
      await evaluate(createDrawing({resolution:ppi}));
      await wait(`!document.querySelector('${drawing}')&&!layerApp.documents.busy()`);
    };
    await invoke('new_document');
    await wait(opened);
    const remembered=(await setup()).resolution;
    assert.ok(remembered>0&&remembered!==600,`New Drawing shows its last resolution: ${remembered}`);
    await cancel();
    await create(600);
    await invoke('image_size');
    await wait('layerApp.state().layer_tools.image_size?.resolution===600');
    await send({type:'image_size',action:{op:'cancel'}});
    await wait('!layerApp.state().layer_tools.image_size');
    await invoke('new_document');
    await wait(opened);
    assert.equal((await setup()).resolution,600,'New Drawing opens with the last drawing\'s resolution');
    await cancel();
    await create(remembered);
  } finally {
    if(theme)await send({type:'set_theme',theme});
  }
}
