export function nativeScreenEvents(events, {screenX,screenY,outerWidth,outerHeight,innerWidth,innerHeight}) {
  const x=screenX+(outerWidth-innerWidth)/2,y=screenY+outerHeight-innerHeight;
  return events.map(event=>event.point?{...event,point:[event.point[0]+x,event.point[1]+y]}:event);
}
