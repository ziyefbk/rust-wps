import {startPresentationShow,stopPresentationShow} from './presentation-show';
import {requestPresentationPdf} from './presentation-pdf';
import {installPresentationSearch} from './presentation-search';
import {wrapTextRuns} from './presentation-wrap';
let previewSerial=0;
export interface Presentation {
  format?: string;
  master_text?: string;
  size_emu: [number, number] | null;
  slides: {id: number; name: string; hidden: boolean; text: string; notes?: string; parse_error: string | null; shapes: Shape[]}[];
  unreadable_parts: [string, string][];
}
interface Run {text: string; font_size_hundredths_pt?: number | null; bold?: boolean | null; italic?: boolean | null; color_rgb?: number[] | null}
interface Shape {vertical_alignment?:string;alignment?:string|null;id: number; name: string; kind: string; bounds_emu: number[] | null; layout_bounds_emu?: number[] | null; master_bounds_emu?: number[] | null; text?: string; runs?: Run[]; image?: string; image_notice?: string; children: Shape[]}
export function closePresentation() { stopPresentationShow();document.getElementById('presentation')!.hidden = true; }
export function openPresentation(report: Presentation, editingMode = false, showReport:()=>Presentation=()=>report) {
  stopPresentationShow();
  const host = document.getElementById('presentation')!;
  host.hidden = false; host.replaceChildren();
  const legacy = report.format === 'ppt';
  if(!editingMode&&!legacy){const importButton=document.createElement('button');importButton.id='pptx-import';importButton.textContent='导入基础编辑副本';importButton.addEventListener('click',()=>document.getElementById('import-document')?.click());host.append(importButton);}
  if (legacy) { const note=document.createElement('p');note.id='legacy-ppt-notice';note.textContent='旧版 PPT：仅逐页文本阅读，备注独立显示；版式、图片、动画和嵌入对象尚未还原。';host.append(note); }
  const editing = document.createElement('div'); editing.className = 'sheet-toolbar';
  const find = document.createElement('input'); find.id = 'pptx-find'; find.setAttribute('aria-label','演示查找文字'); find.placeholder = '查找文字';
  const replacement = document.createElement('input'); replacement.id = 'pptx-replacement'; replacement.setAttribute('aria-label','演示替换文字'); replacement.placeholder = '替换为';
  const save = document.createElement('button'); save.id = 'pptx-replace-save'; save.textContent = '替换并另存 PPTX';
  editing.append(find,replacement,save);if (!editingMode && !legacy) host.append(editing);
  const scope = document.createElement('p'); scope.textContent = '仅替换幻灯片内单个文字节点；不跨格式片段，不修改备注、字段或母版。原文件保留，已有目标不会覆盖。';if (!editingMode && !legacy) host.append(scope);
  const label = document.createElement('label'); label.textContent = '幻灯片 ';
  const picker = document.createElement('select'); picker.id = 'slide-picker'; picker.setAttribute('aria-label', '幻灯片');
  report.slides.forEach((slide, i) => {
    const option = document.createElement('option'); option.value = String(i);
    option.textContent = `${i + 1}. ${slide.name || '幻灯片'}${slide.hidden ? '（隐藏）' : ''}`; picker.append(option);
  });
  label.append(picker); host.append(label);
  if(!editingMode)installPresentationSearch(host,report,picker);
  const play=document.createElement('button');play.id='pptx-play';play.textContent='从当前页放映';play.disabled=legacy||!report.slides.some(slide=>!slide.hidden);play.dataset.unavailable=String(play.disabled);play.addEventListener('click',()=>startPresentationShow(showReport(),Number(picker.value),(host,info,size,shapes,error)=>renderPreview(host,info,size,shapes,error,false)));host.append(play);
  const pdf=document.createElement('button');pdf.id='pptx-export-pdf';pdf.textContent='另存 PDF（近似版面）';pdf.disabled=play.disabled;pdf.dataset.unavailable=String(pdf.disabled);pdf.addEventListener('click',()=>requestPresentationPdf(showReport(),(host,info,size,shapes,error)=>renderPreview(host,info,size,shapes,error,false)));host.append(pdf);
  const pdfScope=document.createElement('p');pdfScope.textContent='PDF 仅导出可见页，使用当前近似画布；复杂图形可能以占位显示，主题、母版、动画等未还原。';pdfScope.hidden=legacy;host.append(pdfScope);
  const status = document.createElement('p'); status.id = 'slide-status'; host.append(status);
  const preview = document.createElement('div'); preview.id = 'slide-preview'; host.append(preview);
  const previewInfo = document.createElement('p'); previewInfo.id = 'slide-preview-info'; host.append(previewInfo);
  const text = document.createElement('pre'); text.id = 'slide-text'; text.style.whiteSpace = 'pre-wrap'; host.append(text);
  const notesPanel=document.createElement('section');notesPanel.hidden=true;
  const notesLabel=document.createElement('p');notesLabel.textContent='演讲者备注';
  const notes=document.createElement('pre');notes.id='slide-notes';notes.style.whiteSpace='pre-wrap';notesPanel.append(notesLabel,notes);host.append(notesPanel);
  const objects = document.createElement('ul'); objects.id = 'slide-objects'; host.append(objects);
  const show = () => {
    const slide = report.slides[Number(picker.value)]; objects.replaceChildren(); preview.replaceChildren();
    notes.textContent=slide?.notes??'';notesPanel.hidden=!slide?.notes;
    if (!slide) { status.textContent = '未列出幻灯片'; text.textContent = ''; return; }
    status.textContent = `${Number(picker.value) + 1} / ${report.slides.length} · ${slide.hidden ? '隐藏页' : '普通页'} · ${report.size_emu?.join(' × ') ?? '尺寸未知'} EMU`;
    text.textContent = slide.parse_error ? `此页读取失败：${slide.parse_error}` : slide.text || '未提取到文字；不代表此页为空。';
    renderPreview(preview, previewInfo, report.size_emu, slide.shapes, slide.parse_error);
    const append = (shapes: Shape[], parent: HTMLElement) => shapes.forEach(shape => {
      const li = document.createElement('li'); li.textContent = `${shape.name || shape.id} · ${shape.kind} · ${shape.bounds_emu?.join(', ') ?? '边界未知'}${shape.image_notice ? ` · ${shape.image_notice}` : ''}`;
      parent.append(li);
      if (shape.children.length) { const nested = document.createElement('ul'); li.append(nested); append(shape.children, nested); }
    });
    append(slide.shapes, objects);
  };
  picker.addEventListener('change', show); show();
  if (report.master_text) {
    const label=document.createElement('p');label.textContent='母版静态文字（整份文稿汇总，不代表每页均适用）';
    const content=document.createElement('pre');content.id='presentation-master-text';content.style.whiteSpace='pre-wrap';content.textContent=report.master_text;
    host.append(label,content);
  }
  if (report.unreadable_parts.length) {
    const warning = document.createElement('p'); warning.textContent = `有 ${report.unreadable_parts.length} 个部件读取失败，内容可能缺失。`; host.append(warning);
  }
}

export function renderPreview(host: HTMLElement, info: HTMLElement, size: Presentation['size_emu'], shapes: Shape[], error: string | null, guides=true) {
  info.textContent = '近似位置预览：支持顶层文字直接样式及内嵌 PNG/JPEG。图片裁剪、组合内容、旋转、主题和母版未还原。';
  if (error || !size || !size.every(n => Number.isSafeInteger(n) && n > 0) || size[0] / size[1] < .1 || size[0] / size[1] > 10) {
    info.textContent = error ? '此页读取失败，无法生成位置预览。' : '幻灯片尺寸缺失或异常，无法生成位置预览。'; return;
  }
  const ns = 'http://www.w3.org/2000/svg';
  const clipPrefix=`slide-preview-${++previewSerial}`;
  const svg = document.createElementNS(ns, 'svg'); svg.id = 'slide-canvas';
  const emuPerPixel = 9525;
  svg.setAttribute('viewBox', `0 0 ${size[0] / emuPerPixel} ${size[1] / emuPerPixel}`);
  svg.setAttribute('role', 'img'); svg.setAttribute('aria-label', '幻灯片近似位置预览');
  svg.style.cssText = 'width:100%;height:auto;display:block;background:white;border:1px solid #c9c4ba;overflow:hidden';
  let skipped = 0, inherited = 0, fromMaster = 0, textOverflow = 0;
  const context=document.createElement('canvas').getContext('2d');
  const family=getComputedStyle(document.body).fontFamily;
  const budget={units:20000,runs:10000},metrics=new Map<string,TextMetrics>();
  const pixels = (run: Run) => typeof run.font_size_hundredths_pt === 'number' && Number.isFinite(run.font_size_hundredths_pt) && run.font_size_hundredths_pt > 0 && run.font_size_hundredths_pt <= 400000 ? run.font_size_hundredths_pt / 75 : 24;
  const measureMetrics=(value:string,run:Run)=>{
    if(!context)return undefined;
    const font=`${run.italic?'italic':'normal'} ${run.bold?'bold':'normal'} ${pixels(run)}px ${family}`,key=font+'\0'+value;
    let measured=metrics.get(key);if(measured===undefined){context.font=font;measured=context.measureText(value);metrics.set(key,measured);}return measured;
  };
  const measure=(value:string,run:Run)=>measureMetrics(value,run)?.width??NaN;
  // Keep the preview bounded independently of the full Rust inventory.
  for (const [shapeIndex, shape] of shapes.slice(0, 500).entries()) {
    const b = shape.bounds_emu ?? shape.layout_bounds_emu ?? shape.master_bounds_emu;
    if (!b || b.length !== 4 || !b.every(Number.isSafeInteger) || b[2] <= 0 || b[3] <= 0) { skipped++; continue; }
    if(!shape.bounds_emu)inherited++;
    if(!shape.bounds_emu&&!shape.layout_bounds_emu)fromMaster++;
    const [x,y,w,h] = b.map(value => value / emuPerPixel);
    const group = document.createElementNS(ns, 'g'); group.dataset.shapeId = String(shape.id);
    const title = document.createElementNS(ns, 'title'); title.textContent = `${shape.name} · ${shape.kind}`; group.append(title);
    const rect = document.createElementNS(ns, 'rect');
    for (const [key,value] of Object.entries({x,y,width:w,height:h})) rect.setAttribute(key,String(value));
    rect.setAttribute('fill','none'); rect.setAttribute('stroke',guides||!['shape','picture'].includes(shape.kind)?'#8c938e':'none'); rect.setAttribute('stroke-width','1'); rect.setAttribute('vector-effect','non-scaling-stroke'); group.append(rect);
    if (shape.kind === 'picture' && shape.image && /^data:image\/(png|jpeg);base64,[A-Za-z0-9+/]+=*$/.test(shape.image) && shape.image.length <= 6 * 1024 * 1024) {
      const picture = document.createElementNS(ns, 'image');
      for (const [key,value] of Object.entries({x,y,width:w,height:h})) picture.setAttribute(key,String(value));
      picture.setAttribute('href',shape.image); picture.setAttribute('preserveAspectRatio','none');
      picture.addEventListener('error',() => { picture.remove(); info.textContent += ' 图片解码失败，请查看内容清单。'; });
      group.append(picture); svg.append(group); continue;
    }
    const clip = document.createElementNS(ns, 'clipPath'); clip.id = `${clipPrefix}-clip-${shapeIndex}`;
    const clipRect = document.createElementNS(ns, 'rect');
    for (const [key,value] of Object.entries({x,y,width:w,height:h})) clipRect.setAttribute(key,String(value));
    clip.append(clipRect); svg.append(clip);
    const anchor=shape.alignment==='ctr'?'middle':shape.alignment==='r'?'end':'start';
    const textX=anchor==='middle'?x+w/2:anchor==='end'?x+w:x;
    const text = document.createElementNS(ns, 'text'); text.setAttribute('x',String(textX)); text.setAttribute('y',String(y + 24));text.setAttribute('text-anchor',anchor);
    text.setAttribute('clip-path',`url(#${clip.id})`);
    text.setAttribute('font-size','24'); text.setAttribute('fill','#263d34');
    text.style.whiteSpace='pre';text.style.fontFamily=family;
    const runs: Run[] = shape.kind === 'shape' ? shape.runs?.length ? shape.runs : [{text:shape.text || ''}] : [{text:`[${shape.kind}：${shape.name}]`}];
    const wrapped=wrapTextRuns(runs,w,measure,budget),lines=wrapped.lines;
    let overflow=wrapped.truncated;
    const heights=lines.map(line=>Math.max(24,...line.map(pixels)));
    let lineBase=0,blockHeight=0;
    for(const [index,line] of lines.entries()){
      lineBase+=heights[index];
      const descent=Math.max(0,...line.map(run=>measureMetrics(run.text,run)?.actualBoundingBoxDescent??0));
      blockHeight=Math.max(blockHeight,lineBase+descent);lineBase+=heights[index]*.2;
    }
    const free=Math.max(0,h-blockHeight);
    let baseline = y+(['middle','ctr'].includes(shape.vertical_alignment??'')?free/2:['bottom','b'].includes(shape.vertical_alignment??'')?free:0);
    for (const line of lines.slice(0, 100)) {
      const height = Math.max(24,...line.map(pixels)); baseline += height;
      for(const run of line){
        if(!run.text.trim())continue;
        const measured=measureMetrics(run.text,run);
        const ascent=measured?.actualBoundingBoxAscent,descent=measured?.actualBoundingBoxDescent;
        // SVG clipping includes glyph ink below the baseline and accents above it.
        if(ascent===undefined||descent===undefined||!Number.isFinite(ascent)||!Number.isFinite(descent)||baseline-ascent<y-1e-6||baseline+descent>y+h+1e-6)overflow=true;
      }
      const lineSpan = document.createElementNS(ns, 'tspan'); lineSpan.setAttribute('x',String(textX)); lineSpan.setAttribute('y',String(baseline));
      for (const run of line) {
        const span = document.createElementNS(ns, 'tspan'); span.textContent = run.text;
        span.setAttribute('font-size',String(pixels(run))); span.setAttribute('font-weight',run.bold === true ? 'bold' : 'normal'); span.setAttribute('font-style',run.italic === true ? 'italic' : 'normal');
        const rgb = run.color_rgb;
        if (rgb?.length === 3 && rgb.every(n => Number.isInteger(n) && n >= 0 && n <= 255)) span.setAttribute('fill',`rgb(${rgb.join(',')})`);
        lineSpan.append(span);
      }
      text.append(lineSpan); baseline += height * .2;
    }
    group.append(text); svg.append(group);
    if(overflow)textOverflow++;
  }
  host.append(svg);
  if(textOverflow){svg.dataset.textOverflow='true';info.textContent+=` ${textOverflow} 个文本框超出高度或自动换行预算，画布可能未显示完整文字；请扩大文本框或减少文字。`;}
  if(inherited)info.textContent += ` ${inherited} 个占位符使用布局或母版位置（其中 ${fromMaster} 个来自母版）；完整样式与变换未恢复。`;
  if (skipped || shapes.length > 500) info.textContent += ` ${skipped + Math.max(0, shapes.length - 500)} 个形状未绘制（边界缺失或预览上限）。`;
}
