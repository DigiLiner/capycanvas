import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';

export async function checkVectorTools({call,evaluate,settle}) {
  const directory=process.env.LAYER_TEST_ARTIFACTS??'artifacts/vector-tools/web';
  await mkdir(directory,{recursive:true});
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();};
  const invoke=command=>send({type:'invoke',command});
  const tool=tool=>send({type:'layer',action:{op:'tool',tool}});
  const screen=([x,y])=>evaluate(`(()=>{const c=layerApp.app.camera(),r=layerApp.canvas.getBoundingClientRect();return{x:r.x+(${x}*c.zoom+c.translation[0])*r.width/c.viewport[0],y:r.y+(${y}*c.zoom+c.translation[1])*r.height/c.viewport[1]}})()`);
  const pen=async(type,at)=>call('Input.dispatchMouseEvent',{type,...await screen(at),button:'left',buttons:type==='mouseReleased'?0:1,clickCount:1,pointerType:'pen',force:.6});
  const stroke=async points=>{
    await pen('mousePressed',points[0]);
    for(const p of points.slice(1)){await pen('mouseMoved',p);await settle();}
    await pen('mouseReleased',points.at(-1));await settle();
  };
  const line=(from,to,steps=16)=>Array.from({length:steps+1},(_,i)=>[from[0]+(to[0]-from[0])*i/steps,from[1]+(to[1]-from[1])*i/steps]);
  const key=async(key,code,windowsVirtualKeyCode)=>{for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key,code,windowsVirtualKeyCode});await settle();};
  const color=async at=>{
    const {x,y}=await screen(at);
    const shot=await call('Page.captureScreenshot',{format:'png',clip:{x:x-2,y:y-2,width:5,height:5,scale:1}});
    return evaluate(`(async()=>{const image=new Image();image.src='data:image/png;base64,${shot.data}';await image.decode();const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;const context=canvas.getContext('2d',{willReadFrequently:true});context.drawImage(image,0,0);const d=context.getImageData(0,0,canvas.width,canvas.height).data;const sum=[0,0,0];for(let i=0;i<d.length;i+=4)for(let c=0;c<3;c++)sum[c]+=d[i+c];return sum.map(v=>v/(d.length/4))})()`);
  };
  const orange=([r,,b])=>r-b>90;
  const green=([r,g,b])=>g-Math.max(r,b)>25;
  const expect=async(name,at,inked,ink=orange)=>{
    const end=Date.now()+20000;let seen;
    do{seen=await color(at);if(ink(seen)===inked)return;await new Promise(r=>setTimeout(r,100));}while(Date.now()<end);
    assert.equal(ink(seen),inked,`${name}: ${seen}`);
  };
  const capture=async name=>writeFile(`${directory}/${name}.png`,Buffer.from((await call('Page.captureScreenshot',{format:'png'})).data,'base64'));
  const selectedLayer=()=>evaluate('Number(layerApp.state().layers.find(l=>l.selected).id)');
  for(const theme of ['light','dark']){
    await send({type:'set_theme',theme});
    await send({type:'layer',action:{op:'new_vector'}});
    await invoke('fit_canvas');
    await send({type:'set_color',rgba:[.85,.35,.1,1]});
    await send({type:'set_brush_size',value:24});

    await tool({figure:{shape:'rectangle',paint:'fill'}});
    await stroke(line([400,400],[700,620]));
    await expect(`${theme} rectangle`,[550,510],true);
    await expect(`${theme} beside the rectangle`,[780,510],false);
    await capture(`vector-${theme}-figure`);

    await tool({figure:{shape:'path',paint:'outline'}});
    for(const at of [[1000,400],[1300,400]])await stroke([at,at]);
    await stroke(line([1300,700],[1400,780],4));
    await capture(`vector-${theme}-path-drafting`);
    await key('Enter','Enter',13);
    await expect(`${theme} path`,[1150,400],true);
    await capture(`vector-${theme}-path`);

    await invoke('edit_points');
    await expect(`${theme} before editing`,[1150,325],false);
    await stroke(line([1000,400],[1000,250]));
    await expect(`${theme} edited anchor`,[1150,325],true);
    await capture(`vector-${theme}-edit-points`);

    await expect(`${theme} before restyling`,[1300,550],false);
    await send({type:'effect',action:{op:'set',layer:await selectedLayer(),key:'path_width',value:{kind:'number',value:160}}});
    await expect(`${theme} restyled`,[1300,550],true);
    await capture(`vector-${theme}-properties`);

    await invoke('move');
    await stroke(line([350,350],[750,670]));
    await capture(`vector-${theme}-frame`);
    await expect(`${theme} before scaling`,[780,700],false);
    await stroke(line([700,620],[850,760]));
    await expect(`${theme} scaled`,[780,700],true);
    await capture(`vector-${theme}-scaled`);

    await send({type:'select_brush',id:1});
    await send({type:'set_color',rgba:[.1,.7,.2,1]});
    await stroke(line([300,1100],[1700,1100],40));
    await expect(`${theme} inked`,[1000,1100],true,green);
    await invoke('eraser');
    await send({type:'set_brush_size',value:40});
    await stroke(line([1000,1000],[1000,1200]));
    await expect(`${theme} erased`,[1000,1100],false,green);
    await expect(`${theme} kept`,[700,1100],true,green);
    await capture(`vector-${theme}-erased`);
    await send({type:'layer',action:{op:'delete_selected'}});
    await invoke('pen');
  }
}
