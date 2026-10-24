import {rgba,json} from './color-wheel.js';

const svgNS='http://www.w3.org/2000/svg';
const node=(tag,attrs={})=>{const n=document.createElementNS(svgNS,tag);for(const [k,v]of Object.entries(attrs))n.setAttribute(k,v);return n;};
const outline=arcs=>arcs.map((a,i)=>{
  const p=t=>[a.center[0]+a.radius*Math.cos(t),a.center[1]+a.radius*Math.sin(t)];
  return `${i?'L':'M'}${p(a.start)} A${a.radius} ${a.radius} 0 ${a.sweep>Math.PI?1:0} 1 ${p(a.start+a.sweep)}`;
}).join(' ')+' Z';

export function bubbleColor(face,body,id,send) {
  face.classList.add('bubble-hdr-face');
  const plate=node('svg',{class:'bubble-color-plate',overflow:'visible','aria-hidden':'true'}),defs=node('defs');
  const filter=node('filter',{id:`bubble-shadow-${id}`,x:'-50%',y:'-50%',width:'200%',height:'200%'}),blur=node('feGaussianBlur',{stdDeviation:8/3});filter.append(blur);
  const mask=node('mask',{id:`bubble-shadow-mask-${id}`,maskUnits:'userSpaceOnUse',x:-1024,y:-1024,width:2048,height:2048}),cut=node('path',{fill:'black'});
  mask.append(node('rect',{x:-1024,y:-1024,width:2048,height:2048,fill:'white'}),cut);defs.append(filter,mask);
  const shadowGroup=node('g',{mask:`url(#bubble-shadow-mask-${id})`}),shadow=node('path',{fill:'black','fill-opacity':.08,filter:`url(#bubble-shadow-${id})`,transform:'translate(0 2)'});
  shadowGroup.append(shadow);const fill=node('path',{fill:'var(--glass-panel)'});plate.append(defs,shadowGroup,fill);face.prepend(plate);
  const arc=node('svg',{class:'bubble-intensity',overflow:'visible'}),ramp=node('g'),track=node('path',{fill:'none',stroke:'transparent','stroke-linecap':'round',role:'slider',tabindex:0});
  const edge=node('circle',{fill:'none',stroke:'rgb(0 0 0 / 50%)','stroke-width':4}),marker=node('circle',{stroke:'white','stroke-width':2});
  arc.append(ramp,track,edge,marker);body.append(arc);
  let data,shapeKey,rampKey,lift;
  track.onkeydown=e=>{
    if(!data?.enabled||!['ArrowLeft','ArrowDown','ArrowRight','ArrowUp','Home'].includes(e.key))return;
    e.preventDefault();send({type:'set_intensity',id,stops:e.key==='Home'?0:Math.max(data.range[0],Math.min(data.range[1],data.stops+(['ArrowLeft','ArrowDown'].includes(e.key)?-.1:.1)))});
  };
  return {
    refresh(color) {
      data=color.intensity;const b=color.bounds,key=json(color.outline);
      Object.assign(arc.style,{left:`${b.x}px`,top:`${b.y}px`,width:`${b.width}px`,height:`${b.height}px`});
      for(const n of [plate,arc]){n.setAttribute('viewBox',`${b.x} ${b.y} ${b.width} ${b.height}`);n.setAttribute('width',b.width);n.setAttribute('height',b.height);}
      if(key!==shapeKey){shapeKey=key;const d=outline(color.outline);for(const n of [cut,shadow,fill])n.setAttribute('d',d);}
      const g=data.geometry,points=data.path;
      track.setAttribute('d',`M${points[0]} A${g.radius} ${g.radius} 0 0 0 ${points.at(-1)}`);track.setAttribute('stroke-width',g.width);
      for(const[k,v]of Object.entries({'aria-label':data.label,'aria-valuetext':data.text,'aria-valuenow':data.stops,'aria-valuemin':data.range[0],'aria-valuemax':data.range[1],'aria-disabled':!data.enabled,tabindex:data.enabled?0:-1}))track.setAttribute(k,v);
      const nextRamp=json([g.width,points,data.ramp]);
      if(nextRamp!==rampKey){rampKey=nextRamp;points.slice(1).forEach((p,i)=>{let line=ramp.children[i];if(!line){line=node('path',{'stroke-linecap':'round'});ramp.append(line);}line.setAttribute('d',`M${points[i]} L${p}`);line.setAttribute('stroke-width',g.width);line.setAttribute('stroke',rgba(data.ramp[i]));});}
      for(const n of [edge,marker]){n.setAttribute('cx',data.point[0]);n.setAttribute('cy',data.point[1]);n.setAttribute('r',g.marker_radius);}
      marker.setAttribute('fill',rgba(data.marker));
    },
    shadow(lifted){if(lift===lifted)return;lift=lifted;blur.setAttribute('stdDeviation',(lifted?12:8)/3);shadow.setAttribute('transform',`translate(0 ${lifted?4.5:2})`);shadow.setAttribute('fill-opacity',lifted?.16:.08);},
    focus(){track.focus({preventScroll:true});},
    dispose(){plate.remove();arc.remove();face.classList.remove('bubble-hdr-face');},
  };
}
