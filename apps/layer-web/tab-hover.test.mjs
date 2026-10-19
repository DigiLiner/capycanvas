import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';

export async function checkTabHoverContours({call,evaluate,settle}) {
  const dir='artifacts/panel-tab-hover/web';await mkdir(dir,{recursive:true});
  const send=async action=>{await evaluate(`layerApp.dispatch(${JSON.stringify(action)})`);await settle();await evaluate('new Promise(r=>setTimeout(r,260))');};
  const move=async point=>{await call('Input.dispatchMouseEvent',{type:'mouseMoved',...point});await settle();};
  const shot=async()=> (await call('Page.captureScreenshot',{format:'png'})).data;
  const saved=await evaluate('layerApp.state().workspace'),fixture=structuredClone(saved),panels=['brushes','sizes','layers','properties'];
  Object.assign(fixture.layout,{bands:[{id:40,edge:'left',extent:300,root:{kind:'tabs',id:41,panels,active:panels[0],tab_style:'icon'}}],floating:[],collapsed:[],column_scroll:[],fit_tab_groups:[],fit_height_groups:[],column_stacks:[],next_id:Math.max(42,fixture.layout.next_id)});fixture.zen_mode=false;
  const inspect=async(selector,active,index,clipped=false)=>{
    const geometry=await evaluate(`(()=>{const n=document.querySelector(${JSON.stringify(selector)}),r=n.getBoundingClientRect(),s=n.parentElement.getBoundingClientRect(),c=getComputedStyle(n);return{rect:{x:r.x,y:r.y,right:r.right,bottom:r.bottom},clip:{x:s.x,right:s.right},color:c.color,radii:[parseFloat(c.borderBottomLeftRadius),parseFloat(c.borderBottomRightRadius)],squircle:['squircle','superellipse(2)'].includes(c.cornerBottomLeftShape||c.cornerShape),published:[Number(n.dataset.hoverLeft),Number(n.dataset.hoverRight)]}})()`);
    const expected=index===active?[0,0]:[index===0?0:index-1===active?6:-6,index===panels.length-1?0:index+1===active?6:-6];
    assert.deepEqual(geometry.published,expected,'neighbor roles determine both lower corners');
    assert.deepEqual(geometry.radii,expected.map(v=>Math.max(0,v)));
    await move({x:800,y:500});const before=await shot();
    const r=geometry.rect,c=geometry.clip;
    await move({x:(Math.max(r.x,c.x)+Math.min(r.right,c.right))/2,y:r.bottom-18});
    assert.equal(await evaluate(`document.querySelector(${JSON.stringify(selector)}).matches(':hover')`),true);
    const after=await shot();
    const comparison=await evaluate(`(async()=>{
      const load=async s=>{const i=new Image();i.src='data:image/png;base64,'+s;await i.decode();const c=new OffscreenCanvas(i.width,i.height),x=c.getContext('2d',{willReadFrequently:true});x.drawImage(i,0,0);return x.getImageData(0,0,c.width,c.height).data};
      const a=await load(${JSON.stringify(before)}),b=await load(${JSON.stringify(after)}),g=${JSON.stringify(geometry)},roles=${JSON.stringify(expected)},selected=${index===active},r=g.rect,clip=g.clip,w=innerWidth,scale=devicePixelRatio,text=g.color.match(/[\\d.]+/g).slice(0,3).map(Number),power=g.squircle?4:2;
      const inside=(x,y)=>{
        if(selected)return false;
        if(x<clip.x||x>=clip.right||y<r.bottom-6||y>=r.bottom)return false;
        if(x>=r.x&&x<r.right){
          for(const [edge,side]of [[r.x,0],[r.right,1]])if(roles[side]>0){const radius=6,dx=side?x-(edge-radius):edge+radius-x,dy=y-(r.bottom-radius);if(dx>0&&dy>0&&(dx/radius)**power+(dy/radius)**power>1)return false;}
          return true;
        }
        const side=x<r.x?0:1,edge=side?r.right:r.x;
        if(roles[side]!==-6||Math.abs(x-edge)>6)return false;
        const dx=side?edge+6-x:x-edge+6,dy=y-r.bottom+6;
        return (dx/6)**4+(dy/6)**4>=1;
      };
      const errors=[];let compared=0,changed=0,untouched=0;
      for(const edge of [r.x,r.right])for(let py=Math.floor((r.bottom-6)*scale);py<Math.ceil(r.bottom*scale);py++)for(let px=Math.floor((edge-6)*scale);px<Math.ceil((edge+6)*scale);px++){
        if(px<0||px>=w*scale||py<0)continue;let coverage=0;for(let sy=0;sy<8;sy++)for(let sx=0;sx<8;sx++)coverage+=inside((px+(sx+.5)/8)/scale,(py+(sy+.5)/8)/scale)?1/64:0;
        const offset=(py*w*scale+px)*4;compared++;let error=0,delta=0;for(let k=0;k<3;k++){error=Math.max(error,Math.abs(b[offset+k]-(a[offset+k]*(1-.08*coverage)+text[k]*.08*coverage)));delta=Math.max(delta,Math.abs(b[offset+k]-a[offset+k]));}if(delta>1)changed++;else untouched++;
        const cx=(px+.5)/scale,cy=(py+.5)/scale,center=inside(cx,cy),boundary=(coverage>0&&coverage<1)||[[-1,-1],[-1,1],[1,-1],[1,1]].some(([dx,dy])=>inside(cx+dx/scale,cy+dy/scale)!==center);
        if(error>(boundary?5:2)&&errors.length<12)errors.push({x:px/scale,y:py/scale,coverage,before:[...a.slice(offset,offset+3)],after:[...b.slice(offset,offset+3)],error});
      }
      return{compared,changed,untouched,errors};})()`);
    assert.deepEqual(comparison.errors,[],`${JSON.stringify({active,index,clipped,expected})}: complete lower corner regions: ${JSON.stringify(comparison)}`);
    if(index===active)assert.equal(comparison.changed,0,'hovering the active tab preserves its feet');else assert.ok(comparison.changed>0);assert.ok(comparison.untouched>0,'active feet and pixels beyond the hover contour stay unchanged');
    return{expected,...comparison};
  };
  try{
    for(const fallback of [false,true])for(const theme of ['light','dark'])for(const drawer of [false,true]){
      await send({type:'restore_workspace',workspace:fixture});await send({type:'set_theme',theme});
      if(drawer){await send({type:'customize',action:{type:'set_column_collapsed',group:41,collapsed:true}});await send({type:'customize',action:{type:'set_column_drawers',column:41,drawers:true}});await evaluate(`document.querySelector('.collapsed-column [data-panel="brushes"]').click()`);await settle();await evaluate('new Promise(r=>setTimeout(r,300))');}
      if(fallback)await evaluate(`(()=>{const s=document.createElement('style');s.id='tab-hover-round-fixture';s.textContent='.dock-tab,.dock-tab::before,.dock-tab::after{corner-shape:round!important}.dock-tab{--corner-fit:.54}';document.head.append(s)})()`);
      const strip=drawer?'.drawer-tab-strip':'.dock-group[data-group="41"] .tab-list';let compared=0,pairs=0;
      const cacheWrites=await evaluate(`(async()=>{const n=document.querySelector(${JSON.stringify(strip)}+' .dock-tab'),original=n.style.setProperty;let writes=0;n.style.setProperty=function(...args){writes++;return original.apply(this,args)};try{const {tabHover}=await import('./workspace-chrome.js');tabHover(n,{hover_bottom:[Number(n.dataset.hoverLeft),Number(n.dataset.hoverRight)]});return writes}finally{n.style.setProperty=original}})()`);
      assert.equal(cacheWrites,0,'unchanged hover roles preserve cached CSS paths');
      for(const active of fallback?[1,2]:[0,1,2,3]){
        await send({type:'select_panel_tab',group:41,panel:panels[active]});
        for(let hover=0;hover<panels.length;hover++){if(fallback&&hover===active)continue;const result=await inspect(`${strip} [data-panel="${panels[hover]}"]`,active,hover);compared+=result.compared;pairs++;}
      }
      await send({type:'select_panel_tab',group:41,panel:'properties'});
      await evaluate(`Object.assign(document.querySelector(${JSON.stringify(strip)}).style,{maxWidth:'84px',flex:'none'});document.querySelector(${JSON.stringify(strip)}).scrollLeft=43`);await settle();
      for(const hover of [1,2])await inspect(`${strip} [data-panel="${panels[hover]}"]`,3,hover,true);
      await send({type:'select_panel_tab',group:41,panel:'brushes'});await evaluate(`document.querySelector(${JSON.stringify(strip)}).scrollLeft=43`);await settle();
      await inspect(`${strip} [data-panel="properties"]`,0,3,true);
      const capture=await shot();await writeFile(`${dir}/${theme}-${drawer?'drawer':'docked'}${fallback?'-round':''}-scrolled.png`,Buffer.from(capture,'base64'));
      console.log(JSON.stringify({theme,drawer,fallback,activeTabs:fallback?2:4,hoverPairs:pairs,compared,cacheWrites,fullLowerCorners:true,activeFeetUntouched:true,stripEnds:true,scrollClipping:true}));
      await evaluate("document.querySelector('#tab-hover-round-fixture')?.remove()");
    }
  }finally{await evaluate("document.querySelector('#tab-hover-round-fixture')?.remove()");await move({x:800,y:500});await send({type:'restore_workspace',workspace:saved});}
}
