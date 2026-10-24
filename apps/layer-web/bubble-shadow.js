const cache=new Map();
const round=(x,y,w,h,r)=>`M${x+r} ${y}h${w-2*r}a${r} ${r} 0 0 1 ${r} ${r}v${h-2*r}a${r} ${r} 0 0 1 ${-r} ${r}h${2*r-w}a${r} ${r} 0 0 1 ${-r} ${-r}v${2*r-h}a${r} ${r} 0 0 1 ${r} ${-r}z`;
function shadow(w,h,r,spread,drop,alpha) {
  r=Math.min(r,w/2,h/2);
  const reach=r+spread,left=r,right=w-r,top=r+drop,bottom=h-r+drop;
  const stops=(a,b)=>[0,.25,.5,.75,1].map(t=>[a+(b-a)*t,alpha*(1-t)**2]);
  const tags=values=>values.map(([offset,opacity])=>`<stop offset="${offset}" stop-opacity="${opacity}"/>`).join('');
  const band=length=>{const edge=spread/(length+2*spread);return tags([...stops(edge,0).reverse(),...stops(1-edge,1)]);};
  const corners=[[left,top,left-reach,top-reach],[right,top,right,top-reach],[left,bottom,left-reach,bottom],[right,bottom,right,bottom]];
  const gradients=corners.map(([cx,cy],i)=>`<radialGradient id="c${i}" gradientUnits="userSpaceOnUse" cx="${cx}" cy="${cy}" r="${reach}">${tags(stops(r/reach,1))}</radialGradient>`).join('');
  const parts=corners.map(([, ,x,y],i)=>`<rect x="${x}" y="${y}" width="${reach}" height="${reach}" fill="url(#c${i})"/>`).join('');
  const image=`<svg xmlns="http://www.w3.org/2000/svg" viewBox="${-spread} ${drop-spread} ${w+2*spread} ${h+2*spread}"><defs><clipPath id="clip"><path clip-rule="evenodd" d="${round(-spread,drop-spread,w+2*spread,h+2*spread,reach)}${round(0,0,w,h,r)}"/></clipPath>${gradients}<linearGradient id="h" x1="0" y1="0" x2="0" y2="1" gradientUnits="objectBoundingBox">${band(h)}</linearGradient><linearGradient id="w">${band(w)}</linearGradient></defs><g clip-path="url(#clip)">${parts}<rect x="${left}" y="${drop-spread}" width="${right-left}" height="${h+2*spread}" fill="url(#h)"/><rect x="${-spread}" y="${top}" width="${w+2*spread}" height="${bottom-top}" fill="url(#w)"/></g></svg>`;
  return `url("data:image/svg+xml,${encodeURIComponent(image)}")`;
}
export function bubbleShadow(node,size,radius,lift=false,bead=false,strength=1) {
  const spread=bead?3:lift?12:8,drop=bead?1:lift?4.5:2,alpha=(bead?.13:lift?.16:.08)/strength;
  const key=[...size,radius,spread,drop,alpha].join(',');
  if(node.bubbleShadowKey===key)return;
  node.bubbleShadowKey=key;
  let image=cache.get(key);
  if(!image){image=shadow(...size,radius,spread,drop,alpha);if(cache.size>=64)cache.delete(cache.keys().next().value);cache.set(key,image);}
  node.style.setProperty('--bubble-shadow',image);
  node.style.setProperty('--bubble-shadow-spread',`${spread}px`);
  node.style.setProperty('--bubble-shadow-top',`${drop-spread}px`);
}
