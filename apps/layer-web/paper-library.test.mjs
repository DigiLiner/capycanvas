import assert from 'node:assert/strict';

export async function checkPaperLibrary({evaluate,settle}) {
  const send=async value=>{await evaluate(`layerApp.dispatch(${JSON.stringify(value)})`);await settle();};
  const wait=(condition,timeout=30000)=>evaluate(`new Promise((resolve,reject)=>{const end=performance.now()+${timeout};function poll(){if(${condition})resolve();else if(performance.now()>end)reject(Error(${JSON.stringify(condition)}));else setTimeout(poll,20)}poll()})`);
  const texture="layerApp.state().layer_properties.controls.find(c=>c.key==='texture')";
  const library=`(${texture}?.paper?.tiles??[]).flatMap(t=>t.actions).map(a=>a.action).filter(a=>a.op==='rename_paper')`;
  const idle="!layerApp.state().requests.some(r=>r.kind.type==='paper')";
  const revision=()=>evaluate('Number(layerApp.state().document_file.revision)');
  const paper=await evaluate(`(()=>{for(const l of layerApp.state().layers){layerApp.dispatch({type:'select_layer',id:l.id});if(${texture})return Number(l.id);}return null})()`);
  assert.ok(paper!==null,'The default drawing has a Paper layer');
  await wait(`!!${texture}?.paper?.import`);
  for(const action of await evaluate(library))await send({type:'effect',action:{op:'delete_paper',id:action.id}});
  await wait(`${idle}&&${library}.length===0`);
  await evaluate(`(async()=>{
    const canvas=new OffscreenCanvas(64,48),context=canvas.getContext('2d');
    for(let y=0;y<48;y++)for(let x=0;x<64;x++){const v=(x*7+y*3)%256;context.fillStyle='rgb('+v+','+v+','+v+')';context.fillRect(x,y,1,1);}
    const file=new File([await canvas.convertToBlob({type:'image/png'})],'Linen scan.png',{type:'image/png'});
    const click=HTMLInputElement.prototype.click;
    HTMLInputElement.prototype.click=function(){
      if(this.type!=='file'||this.accept!=='image/*')return click.call(this);
      HTMLInputElement.prototype.click=click;window.paperChooser=this.accept;
      const transfer=new DataTransfer();transfer.items.add(file);this.files=transfer.files;this.dispatchEvent(new Event('change'));
    };
  })()`);
  await send({type:'effect',action:{op:'import_paper',layer:paper}});
  await wait(`${idle}&&${library}.length===1`);
  assert.equal(await evaluate('window.paperChooser'),'image/*','Import chooses an image');
  assert.equal(await evaluate('layerApp.state().document_file.modified'),true,'Import applies the new paper');
  assert.equal(await evaluate(`${texture}.paper.tiles.some(t=>t.label==='Linen scan')`),true,'The imported file name names the paper');
  const id=(await evaluate(library))[0].id,imported=await revision();
  await send({type:'effect',action:{op:'rename_paper',id,name:'Cotton rag'}});
  await wait(`${idle}&&${texture}.paper.tiles.some(t=>t.label==='Cotton rag')`);
  await evaluate('layerApp.documents.loadPapers()');
  await wait(`${idle}&&${texture}.paper.tiles.some(t=>t.label==='Cotton rag')`);
  await send({type:'effect',action:{op:'delete_paper',id}});
  await wait(`${idle}&&${library}.length===0`);
  assert.equal(await revision(),imported,'Library edits leave the drawing unchanged');
  assert.equal(await evaluate('layerApp.state().host_error??null'),null);
  console.log('PASS: paper library import, rename, reload and delete');
}
