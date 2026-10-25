import assert from 'node:assert/strict';

export async function checkCanvasCancellation(fixture, {call,evaluate,settle}) {
  const {send,idle,checkpoint,documentPoint,invoke}=fixture;
  for(const theme of ['light','dark']) {
  await send({type:'set_theme',theme});
  for(const tool of [{figure:{shape:'rectangle',paint:'fill'}},'gradient','lasso_fill']) {
    if(tool==='gradient')await invoke(tool);else await send({type:'layer',action:{op:'tool',tool}});await idle();
    const before=await checkpoint(),points=await Promise.all([[60,60],[190,60],[190,190]].map(documentPoint));
    const mouse=async(type,p,buttons)=>{await call('Input.dispatchMouseEvent',{type,...p,button:'left',buttons,pointerType:'pen',force:.7});await settle();};
    const path=async()=>{await mouse('mousePressed',points[0],1);for(const p of points.slice(1))await mouse('mouseMoved',p,1);};
    await evaluate(`layerApp.canvas.addEventListener('pointerdown',e=>{window.cancelPointer=e.pointerId},{once:true})`);
    await path();
    await evaluate(`(()=>{const c=layerApp.canvas; c.dispatchEvent(new PointerEvent('pointercancel',{bubbles:true,pointerId:window.cancelPointer,pointerType:'pen',buttons:0,pressure:0}));})()`);
    await mouse('mouseReleased',points.at(-1),0);await idle();
    assert.deepEqual(await checkpoint(),before,`${JSON.stringify(tool)} explicit pen cancellation preserves authored content and raster identity`);
    await path();await mouse('mouseReleased',points.at(-1),0);await idle();
    assert.notDeepEqual(await checkpoint(),before,`${JSON.stringify(tool)} ordinary pen release commits`);
    await invoke('undo');await idle();assert.deepEqual(await checkpoint(),before,'normal release adds one undo step');
  }
  }
}
