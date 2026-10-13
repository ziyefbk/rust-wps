import {invoke,isTauri} from '@tauri-apps/api/core';
import type {Presentation} from './presentation';
type Renderer=(host:HTMLElement,info:HTMLElement,size:Presentation['size_emu'],shapes:Presentation['slides'][number]['shapes'],error:string|null)=>void;
let operation:(label:string,action:()=>Promise<void>)=>Promise<void>;
let notice:(text:string,error?:boolean)=>void=()=>{};
export function initializePresentationPdf(run:typeof operation,show:typeof notice){operation=run;notice=show;}
export function clearPresentationPrint(){document.getElementById('print-document')!.replaceChildren();document.getElementById('presentation-print-style')?.remove();}
export async function preparePresentationPrint(source:Presentation,render:Renderer){
  clearPresentationPrint();const report=structuredClone(source),size=report.size_emu;
  if(!size||!size.every(n=>Number.isSafeInteger(n)&&n>=914400&&n<=91440000))throw new Error('演示 PDF 要求有效的 1–100 英寸页面尺寸');
  const slides=report.slides.filter(slide=>!slide.hidden);
  if(!slides.length||slides.length>1000)throw new Error('演示 PDF 要求 1–1000 张可见页');
  if(slides.reduce((n,s)=>n+s.shapes.length,0)>10000)throw new Error('演示 PDF 对象数量超过 10000');
  const width=size[0]/914400,height=size[1]/914400;
  const pages=document.createElement('div');pages.id='presentation-pdf-pages';let bytes=0;
  for(const slide of slides){
    if(slide.parse_error)throw new Error('有可见页读取失败，未生成 PDF');
    if(slide.shapes.length>500||slide.shapes.some(s=>{
      const bounds=s.bounds_emu??s.layout_bounds_emu??s.master_bounds_emu;
      return !bounds||bounds.length!==4||!bounds.every(Number.isSafeInteger)||bounds[2]<=0||bounds[3]<=0||(s.text??'').split('\n').length>100;
    }))throw new Error('有对象超出近似 PDF 绘制范围，请先核对文稿');
    const page=document.createElement('section');page.className='pptx-pdf-page';const info=document.createElement('p');render(page,info,size,slide.shapes,null);
    const svg=page.querySelector('svg');if(!svg)throw new Error('页面无法绘制，未生成 PDF');
    if(svg.dataset.textOverflow==='true')throw new Error('文字超出文本框高度或自动换行预算，未生成 PDF；请调整文本框');
    svg.removeAttribute('id');svg.style.fontFamily=getComputedStyle(document.body).fontFamily;
    for(const image of Array.from(svg.querySelectorAll('image'))){
      const data=image.getAttribute('href')??'';if(!/^data:image\/(png|jpeg);base64,[A-Za-z0-9+/]+=*$/.test(data)||data.length>6*1024*1024)throw new Error('PDF 图片必须为内嵌 PNG/JPEG');
      const decoded=new Image();decoded.src=data;await decoded.decode();
    }
    bytes+=new TextEncoder().encode(page.outerHTML).length;if(bytes>16*1024*1024)throw new Error('演示 PDF 快照超过 16 MiB');
    pages.append(page);
  }
  await document.fonts.ready;
  const style=document.createElement('style');style.id='presentation-print-style';style.textContent=`@page {size:${width}in ${height}in;margin:0;} @media print {#print-document{font-size:0;line-height:0;overflow-wrap:normal;} #print-document .pptx-pdf-page{width:${width}in;height:${height}in;overflow:hidden;break-after:page;break-inside:avoid;margin:0;padding:0;box-sizing:border-box;} #print-document .pptx-pdf-page:last-child{break-after:auto;} #print-document .pptx-pdf-page svg{width:100%!important;height:100%!important;border:0!important;display:block;}}`;
  document.head.append(style);document.getElementById('print-document')!.replaceChildren(pages);
  return {width_inches:width,height_inches:height,expected_pages:slides.length};
}
export function requestPresentationPdf(report:Presentation,render:Renderer){
  if(!isTauri())return;
  void operation('正在导出演示 PDF…',async()=>{
    try{
      const page=await preparePresentationPrint(report,render);
      const saved=await invoke<{path:string}|null>('export_current_pdf',{page});
      notice(saved?`已另存近似 PDF：${saved.path}。PPTX 中的修改仍需另存。`:'已取消 PDF 导出，演示文稿保留。');
    }finally{clearPresentationPrint();}
  });
}
