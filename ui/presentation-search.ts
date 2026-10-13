import type {Presentation} from './presentation';
export interface SlideMatch {slide:number;field:'text'|'notes';offset:number}
export function searchPresentation(report:Presentation,query:string):{matches:SlideMatch[];truncated:boolean} {
  if(!query || query.length>256)return {matches:[],truncated:false};
  const matches:SlideMatch[]=[];let bytes=0;
  for(let slide=0;slide<Math.min(report.slides.length,1000);slide++){
    const page=report.slides[slide];if(page.parse_error)continue;
    for(const field of ['text','notes'] as const){
      const text=page[field]??'';bytes+=new TextEncoder().encode(text).length;
      if(bytes>4*1024*1024)return {matches,truncated:true};
      let offset=0;
      while((offset=text.indexOf(query,offset))!==-1){
        if(matches.length===1000)return {matches,truncated:true};
        matches.push({slide,field,offset});offset+=query.length;
      }
    }
  }
  return {matches,truncated:report.slides.length>1000};
}

export function installPresentationSearch(host:HTMLElement,report:Presentation,picker:HTMLSelectElement){
  const panel=document.createElement('div');panel.className='sheet-toolbar';
  const query=document.createElement('input');query.id='slide-search';query.maxLength=256;query.setAttribute('aria-label','跨页查找正文和备注');query.placeholder='跨页查找（区分大小写）';
  const run=document.createElement('button');run.id='slide-search-run';run.textContent='查找';
  const results=document.createElement('select');results.id='slide-search-results';results.setAttribute('aria-label','演示查找结果');
  const status=document.createElement('span');status.id='slide-search-status';status.setAttribute('aria-live','polite');
  const disable=()=>{results.disabled=true;results.dataset.unavailable='true';};disable();
  let matches:SlideMatch[]=[];
  const jump=()=>{const match=matches[Number(results.value)];if(!match)return;picker.value=String(match.slide);picker.dispatchEvent(new Event('change'));
    status.textContent=`第 ${match.slide+1} 页 · ${match.field==='text'?'正文':'备注'} · 字符位置 ${match.offset+1}${report.slides[match.slide].hidden?' · 隐藏页':''}`;
  };
  const search=()=>{const result=searchPresentation(report,query.value);matches=result.matches;results.replaceChildren();
    for(const [i,match] of matches.entries()){const option=document.createElement('option');option.value=String(i);option.textContent=`${i+1}. 第 ${match.slide+1} 页 · ${match.field==='text'?'正文':'备注'}${report.slides[match.slide].hidden?'（隐藏）':''}`;results.append(option);}
    results.disabled=!matches.length;results.dataset.unavailable=String(results.disabled);
    status.textContent=query.value?`${matches.length} 个匹配${result.truncated?'（达到查找上限，结果不完整）':''}`:'请输入查找文字';
    if(matches.length)jump();
    if(result.truncated)status.textContent+=' · 结果达到上限，未完整列出';
  };
  run.addEventListener('click',search);results.addEventListener('change',jump);
  query.addEventListener('keydown',event=>{if(event.key==='Enter'){event.preventDefault();search();}});
  query.addEventListener('input',()=>{matches=[];results.replaceChildren();disable();status.textContent='';});
  panel.append(query,run,results,status);host.append(panel);
}
