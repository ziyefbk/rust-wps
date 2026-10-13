import type {Presentation} from './presentation';
type Renderer=(host:HTMLElement,info:HTMLElement,size:Presentation['size_emu'],shapes:Presentation['slides'][number]['shapes'],error:string|null)=>void;
let stop:(()=>void)|null=null;
export function stopPresentationShow(){stop?.();}
export function startPresentationShow(report:Presentation,start:number,render:Renderer){
  stopPresentationShow();
  const pages=report.slides.map((slide,index)=>({slide,index})).filter(p=>!p.slide.hidden);
  if(!pages.length)return;
  let current=pages.findIndex(p=>p.index===start);if(current<0)current=0;
  const previous=document.activeElement as HTMLElement|null;
  const dialog=document.createElement('dialog');dialog.id='presentation-show';dialog.className='presentation-show';dialog.setAttribute('aria-label','幻灯片放映');dialog.tabIndex=-1;
  const surface=document.createElement('div');surface.id='presentation-show-surface';surface.className='presentation-show-surface';dialog.append(surface);
  const bar=document.createElement('div');bar.className='presentation-show-bar';surface.append(bar);
  const count=document.createElement('span');count.id='pptx-show-status';count.setAttribute('aria-live','polite');bar.append(count);
  const button=(id:string,text:string,action:()=>void)=>{const b=document.createElement('button');b.id=id;b.textContent=text;b.addEventListener('click',action);bar.append(b);return b;};
  const back=button('pptx-show-prev','上一页',()=>move(current-1));
  const next=button('pptx-show-next','下一页',()=>move(current+1));
  const fullscreen=button('pptx-show-fullscreen','全屏',()=>void(async()=>{
    try{if(document.fullscreenElement===surface)await document.exitFullscreen();else await surface.requestFullscreen();}
    catch{feedback.textContent='当前环境无法进入系统全屏，仍可在此窗口放映。';}
  })());
  button('pptx-show-exit','退出放映 Esc',()=>stopPresentationShow());
  const stage=document.createElement('div');stage.id='pptx-show-stage';stage.className='presentation-show-stage';stage.setAttribute('aria-label','当前放映画面');surface.append(stage);
  const info=document.createElement('p');info.id='pptx-show-info';info.className='presentation-show-info';surface.append(info);
  const feedback=document.createElement('p');feedback.id='pptx-show-feedback';feedback.className='presentation-show-info';feedback.textContent='←/→、PageUp/PageDown 翻页，Home/End 跳转，Esc 退出；点击画面前进。';surface.append(feedback);
  const fit=()=>{const svg=stage.querySelector('svg');if(!svg)return;const box=svg.viewBox.baseVal,scale=Math.min(stage.clientWidth/box.width,stage.clientHeight/box.height);if(scale>0&&Number.isFinite(scale)){svg.style.width=`${box.width*scale}px`;svg.style.height=`${box.height*scale}px`;}};
  const observer=new ResizeObserver(fit);observer.observe(stage);
  function move(index:number){
    current=Math.max(0,Math.min(index,pages.length-1));const {slide,index:original}=pages[current];stage.replaceChildren();
    count.textContent=`${current+1} / ${pages.length} · 原第 ${original+1} 页${pages.length<report.slides.length?' · 已跳过隐藏页':''}`;
    back.disabled=current===0;next.disabled=current===pages.length-1;
    render(stage,info,report.size_emu,slide.shapes,slide.parse_error);
    const svg=stage.querySelector('svg');
    if(svg){svg.id='pptx-show-canvas';svg.setAttribute('aria-label',`放映第 ${original+1} 页`);fit();}
    else{const fallback=document.createElement('pre');fallback.className='presentation-show-fallback';fallback.textContent=slide.text||'此页无法绘制，未提取到可显示文字。';stage.append(fallback);}
  }
  stage.addEventListener('click',()=>move(current+1));
  dialog.addEventListener('keydown',event=>{
    event.stopPropagation();
    if(event.key==='Escape'){event.preventDefault();stopPresentationShow();return;}
    if(event.ctrlKey||event.metaKey){if(event.key.toLowerCase()!=='c')event.preventDefault();return;}
    const key=event.key;
    if((key===' '||key==='Enter')&&event.target instanceof HTMLButtonElement)return;
    if(['ArrowRight','ArrowDown','PageDown',' ','Enter'].includes(key)){event.preventDefault();move(current+1);}
    else if(['ArrowLeft','ArrowUp','PageUp'].includes(key)){event.preventDefault();move(current-1);}
    else if(key==='Home'||key==='End'){event.preventDefault();move(key==='Home'?0:pages.length-1);}
  });
  const fullscreenChange=()=>{fullscreen.textContent=document.fullscreenElement===surface?'退出全屏':'全屏';};
  document.addEventListener('fullscreenchange',fullscreenChange);
  const teardown=()=>{
    if(stop!==teardown)return;stop=null;
    document.removeEventListener('fullscreenchange',fullscreenChange);
    observer.disconnect();
    if(document.fullscreenElement===surface)void document.exitFullscreen().catch(()=>{});
    dialog.close();dialog.remove();if(previous?.isConnected)previous.focus();
  };
  stop=teardown;
  dialog.addEventListener('cancel',event=>{event.preventDefault();teardown();});dialog.addEventListener('close',teardown);
  document.body.append(dialog);move(current);dialog.showModal();dialog.focus();
}
