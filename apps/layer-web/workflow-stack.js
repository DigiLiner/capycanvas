export function createWorkflowStack(root, transition = 'crossfade') {
  root.classList.add('workflow-stack');root.dataset.transition=transition;
  let current=null,finish=null;
  function show(next) {
    if(current===next)return;
    finish?.();
    const previous=current;current=next;
    const pages=[...root.children];
    for(const page of pages){page.classList.add('workflow-page');page.hidden=page!==next;page.inert=page!==next;page.classList.toggle('active',page===next);}
    const reduced=globalThis.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
    if(!previous||reduced||!root.getBoundingClientRect||!globalThis.requestAnimationFrame)return;
    next.hidden=true;previous.hidden=false;
    const before=root.getBoundingClientRect();
    previous.hidden=true;next.hidden=false;
    const after=root.getBoundingClientRect();
    if(!before.width||!after.width)return;
    root.style.width=`${before.width}px`;root.style.height=`${before.height}px`;
    root.style.setProperty('--workflow-direction',pages.indexOf(next)>pages.indexOf(previous)?'1':'-1');
    previous.hidden=false;previous.classList.add('leaving');next.classList.add('entering');root.classList.add('transitioning');
    let timer,frame;
    const done=()=>{
      clearTimeout(timer);cancelAnimationFrame(frame);
      root.classList.remove('transitioning');root.style.width='';root.style.height='';
      for(const page of pages){page.classList.remove('entering','leaving');page.hidden=page!==current;page.inert=page!==current;page.classList.toggle('active',page===current);}
      finish=null;
    };
    finish=done;
    root.getBoundingClientRect();
    frame=requestAnimationFrame(()=>{
      root.style.width=`${after.width}px`;root.style.height=`${after.height}px`;next.classList.remove('entering');
      const duration=globalThis.getComputedStyle?.(root).transitionDuration??'0.2s';
      const milliseconds=Math.max(...duration.split(',').map(value=>parseFloat(value)*(value.trim().endsWith('ms')?1:1000)))||0;
      timer=setTimeout(done,milliseconds+32);
    });
  }
  return {show,finish:()=>finish?.()};
}
