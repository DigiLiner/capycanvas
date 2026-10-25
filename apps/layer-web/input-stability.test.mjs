import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {scopesFixture} from './scopes-fixture.mjs';
import {createDrawing} from './new-drawing-fixture.mjs';

export async function checkInputStability({call,evaluate,settle}) {
  const fixture=await scopesFixture({call,evaluate,settle});
  const {send,invoke,poll,checkpoint,reopen,dispose}=fixture;
  const directory=process.env.LAYER_TEST_ARTIFACTS??'artifacts/input-stability/web',results=[];
  await mkdir(directory,{recursive:true});
  const drain=async()=>{await settle();await poll('!layerApp.app.canvas_work_pending()&&layerApp.app.document_park_ready()');await evaluate('layerApp.app.wait_for_canvas()');};
  const pen=async records=>{
    const result=await evaluate(`(()=>{const c=layerApp.app.camera(),now=performance.now(),records=${records};for(const item of records){item[2]=item[2]*c.zoom+c.translation[0];item[3]=item[3]*c.zoom+c.translation[1];item[8]+=now;}try{return{count:layerApp.app.pen(new Float64Array(records.flat()),BigInt(layerApp.state().camera.revision)),error:null}}catch(error){return{error:String(error)}}finally{layerApp.wake()}})()`);
    results.push(result);await settle();return result;
  };
  const record=(phase,x,y,time,flags=2,pressure=1)=>`[8101,${phase},${x},${y},${pressure},0,0,0,${time},${flags},0]`;
  const clean=y=>`[${record(1,80,y,0)},${record(2,160,y,10)},${record(3,240,y,20,2,0)}]`;
  try {
    await invoke('new_document');await evaluate(createDrawing({width:320,height:240}));await drain();await invoke('fit_canvas');
    await invoke('pen');await send({type:'select_brush',id:1});await send({type:'set_brush_size',value:32});await send({type:'set_color',rgba:[0,0,0,1]});await drain();
    const before=await checkpoint();
    const mixed=await pen(`[${record(1,80,80,0)},${record(2,'NaN',80,5)},${record(2,160,80,10)},${record(3,240,80,20,2,0)}]`);
    if(mixed.error)await pen(`[${record(4,240,80,25,2,0)}]`);
    await drain();
    const painted=await checkpoint();
    await writeFile(`${directory}/mixed-batch.json`,JSON.stringify({result:mixed,before,after:painted},null,2));
    assert.notDeepEqual(painted.rasters,before.rasters,'valid siblings after a non-finite record commit their stroke');
    assert.equal(mixed.error,null,'a bad sample does not abort the public pen batch');
    await invoke('undo');await drain();assert.deepEqual(await checkpoint(),before,'mixed batch has one exact Undo step');
    await invoke('redo');await drain();assert.deepEqual(await checkpoint(),painted,'mixed batch has exact Redo');

    const canceledBefore=await checkpoint();
    await pen(`[${record(1,80,140,0)},${record(2,160,140,10)}]`);
    await pen(`[${record(3,240,140,20,2,'NaN')}]`);
    await pen(`[${record(2,220,140,25)},${record(3,240,140,30,2,0)}]`);await drain();
    assert.deepEqual(await checkpoint(),canceledBefore,'an invalid real Up cancels contact and later Move/Up cannot commit it');
    await invoke('undo');await drain();assert.deepEqual(await checkpoint(),before,'canceled contact adds no history step');
    await invoke('redo');await drain();assert.deepEqual(await checkpoint(),painted);
    await reopen();assert.deepEqual(await checkpoint(),painted,'mixed batch native pixels survive save/reopen');

    for(const flags of [3,34]) {
      const prior=await checkpoint();
      const result=await pen(`[${record(1,80,180,0)},${record(3,'NaN',180,5,flags)},${record(2,160,180,10)},${record(3,240,180,20,2,0)}]`);await drain();
      assert.equal(result.error,null,`invalid supplemental flags ${flags} do not abort valid siblings`);
      assert.notDeepEqual((await checkpoint()).rasters,prior.rasters,`invalid supplemental flags ${flags} preserve the physical contact`);
      await invoke('undo');await drain();assert.deepEqual(await checkpoint(),prior,`flags ${flags}: exact Undo`);
    }
    await pen(clean(180));await drain();assert.notDeepEqual((await checkpoint()).rasters,painted.rasters,'a new valid stroke succeeds after canceled and supplemental records');
    await reopen();await writeFile(`${directory}/input-stability.json`,JSON.stringify({results},null,2));
    console.log('PASS public pen batches: valid siblings, malformed release cancellation, prediction/correction isolation, exact Undo/Redo and save/reopen');
  } finally {await dispose();}
}
