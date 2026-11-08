import assert from 'node:assert/strict';
import {readyCanvas,waitForPixels} from './canvas-pixels-fixture.mjs';
import {chooseDropdown} from './dropdown-fixture.mjs';
// An in-page script that fills the open New drawing dialog through its
// controls and, unless `create` is false, presses Create. Sizes are in
// pixels; `paper` is 'white', 'transparent' or null to keep the dialog's
// paper color; `relief` is the Texture visibility in percent or null to keep
// the dialog's.
export function createDrawing({width = 2048, height = 1536, resolution = null, space = null, depth = null, paper = 'white', relief = 0, name = null, create = true} = {}) {
  return `(async()=>{
    const end=performance.now()+30000,dialog=()=>document.querySelector('#new-drawing-dialog[open]'),view=()=>layerApp.state().new_drawing;
    while(!dialog()||!view()){if(performance.now()>end)throw Error('New drawing did not open');await new Promise(r=>setTimeout(r,20));}
    const choose=(id,key,value)=>{const index=view()[key].findIndex(c=>c.value===value);if(index<0)throw Error('No '+key+' '+value);(${chooseDropdown.toString()})('#'+id,String(index));};
    const type=(selector,text)=>{const n=dialog().querySelector(selector);n.focus();n.value=String(text);n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));};
    const press=selector=>{const n=dialog().querySelector(selector);if(!n||n.disabled)throw Error('Cannot press '+selector);n.click();};
    choose('new-drawing-unit','units','px');
    type('#new-drawing-width input',${width});type('#new-drawing-height input',${height});
    ${resolution == null ? '' : `type('#new-drawing-resolution-field input',${resolution});`}
    ${space == null ? '' : `choose('new-drawing-space','spaces',${JSON.stringify(space)});`}
    ${depth == null ? '' : `choose('new-drawing-depth','depths',${JSON.stringify(depth)});`}
    ${paper == null ? '' : `press('#new-drawing-paper-color');press(${JSON.stringify(paper === 'white' ? '#new-drawing-paper-color-tiles [data-new-drawing-paper-color="0"]' : '#new-drawing-color-mode-transparent')});press('#new-drawing-color-use');`}
    ${relief == null ? '' : `press('#new-drawing-texture');type('#new-drawing-paper-relief-field input',${relief});press('#new-drawing-paper-use');if(view().setup.paper.relief!==${relief})throw Error('New drawing kept relief '+view().setup.paper.relief);`}
    ${name == null ? '' : `const field=dialog().querySelector('#new-drawing-name');field.value=${JSON.stringify(name)};field.dispatchEvent(new Event('input',{bubbles:true}));`}
    const setup=view().setup;
    if(setup.unit!=='px'||view().values[0]!==${width}||view().values[1]!==${height})throw Error('New drawing kept '+JSON.stringify(view().values));
    if(!${create})return setup;
    press('#new-drawing-create');
    while(view()){if(performance.now()>end)throw Error('New drawing did not create');await new Promise(r=>setTimeout(r,20));}
    return setup;
  })()`;
}

export async function checkPngPaperRows({call,evaluate,settle,wait,click,capture},theme) {
  const tabs=await evaluate('layerApp.app.document_tabs(0).tabs.length');
  await evaluate(`(async()=>{const canvas=new OffscreenCanvas(128,64),context=canvas.getContext('2d');context.fillStyle='#468CD2';context.fillRect(0,0,128,64);const blob=await canvas.convertToBlob({type:'image/png'});window.paperOpenPicker=window.showOpenFilePicker;window.showOpenFilePicker=async()=>[{name:'paper-check.png',async getFile(){return new File([blob],'paper-check.png')}}];layerApp.dispatch({type:'preferences',action:{type:'edit',id:'missing_profile',value:0}});layerApp.dispatch({type:'invoke',command:'open_document'});})()`);
  try {
    await wait(`!layerApp.documents.busy()&&!layerApp.state().document_file.busy&&layerApp.state().tabs[0].width===128&&layerApp.app.document_tabs(0).tabs.length===${tabs+1}&&layerApp.app.brush_ready()`,60000);
    await readyCanvas({evaluate,settle});
    assert.equal(await evaluate('layerApp.state().layers.some(l=>l.paper)'),false,'Opening PNG creates no Paper layer');
    await wait(`!!document.querySelector('#layer-add-paper')`);
    const pixel=async()=>{
      const {data}=await call('Page.captureScreenshot',{format:'png'});
      return evaluate(`(async()=>{const image=new Image();image.src='data:image/png;base64,${data}';await image.decode();const canvas=new OffscreenCanvas(image.width,image.height),context=canvas.getContext('2d'),bounds=document.querySelector('#canvas').getBoundingClientRect();context.drawImage(image,0,0);const camera=layerApp.state().camera,scale=image.width/innerWidth,x=bounds.x+(camera.translation[0]+64*camera.zoom)*bounds.width/camera.viewport[0],y=bounds.y+(camera.translation[1]+32*camera.zoom)*bounds.height/camera.viewport[1];return Array.from(context.getImageData(Math.floor(x*scale),Math.floor(y*scale),1,1).data);})()`);
    };
    await waitForPixels(pixel,settle,pixels=>pixels.every((value,index)=>Math.abs(value-[70,140,210,255][index])<=8),'The opened PNG is presented before Add Paper');
    await capture(`${theme}-png-no-paper`);
    await click('#layer-add-paper');
    await wait(`layerApp.state().layers.some(l=>l.paper)&&!document.querySelector('#layer-add-paper')`);
    await readyCanvas({evaluate,settle});
    await capture(`${theme}-png-add-paper`);
    await evaluate('layerApp.documents.close(layerApp.app.document_tabs(0).selected)');
    await wait("!![...document.querySelectorAll('dialog[open] button')].find(button=>button.textContent==='Discard Changes')");
    await evaluate("[...document.querySelectorAll('dialog[open] button')].find(button=>button.textContent==='Discard Changes').click()");
    await wait(`layerApp.app.document_tabs(0).tabs.length===${tabs}&&!layerApp.documents.busy()&&layerApp.app.brush_ready()`,60000);
  } finally {await evaluate('window.showOpenFilePicker=paperOpenPicker;delete window.paperOpenPicker');}
}
