import {PresentationHistory,type PresentationSnapshot} from './presentation-history';
import {invoke,isTauri} from '@tauri-apps/api/core';
import {resizePresentation} from './presentation-resize';
import {openPresentation,renderPreview,type Presentation as View} from './presentation';
export interface PresentationModel {version:1;size_emu:[number,number];slides:{text_boxes:TextBox[];images?:Picture[];notes?:string;hidden?:boolean}[]}
interface Picture {bounds_emu:[number,number,number,number];data_url:string;alt:string}
interface TextBox {vertical_alignment?:'top'|'middle'|'bottom';alignment?:'left'|'center'|'right';bounds_emu:[number,number,number,number];text:string;font_size_hundredths_pt:number;bold:boolean;italic:boolean;color_rgb:[number,number,number]}
let active:{model:PresentationModel;dirty:boolean;revision:number;draftRevision:number;draftFailed:boolean;history:PresentationHistory;committed:PresentationSnapshot;saved:string|null}|null=null;
let draftTimer:ReturnType<typeof setTimeout>|null=null;
let draftQueue:Promise<void>=Promise.resolve();
let leaving=false;
let saving=false;
let paused=false;
export function pausePresentation(value:boolean){paused=value;}
let showNotice:(text:string,error?:boolean)=>void=()=>{};
let operation:(label:string,action:()=>Promise<void>)=>Promise<void>;
export function initializePresentationEditor(notice:typeof showNotice,run:typeof operation){showNotice=notice;operation=run;}
function draftStatus(text:string){const el=document.getElementById('presentation-edit-state');if(el)el.textContent=text;}
function queueDraft(){if(draftTimer)clearTimeout(draftTimer);draftTimer=setTimeout(()=>{draftTimer=null;void writeDraft().catch(()=>{});},700);}
function writeDraft(){
  const owner=active;if(!owner)return draftQueue;
  const snapshot=structuredClone(owner.model), revision=owner.revision;
  const next=draftQueue.catch(()=>{}).then(async()=>{
    if(!isTauri())throw new Error('浏览器预览不能保存本地演示草稿');
    await invoke('save_presentation_draft',{model:snapshot});
    owner.draftRevision=revision;owner.draftFailed=false;
    if(active===owner&&revision===owner.revision)draftStatus(owner.dirty?'本地草稿已保存 · 有未另存 PPTX 的修改':'本地草稿已保存 · PPTX 已另存');
  }).catch(error=>{owner.draftFailed=true;if(active===owner){draftStatus('草稿保存失败 · 修改仍在内存');showNotice(`演示草稿保存失败：${String(error)}`,true);}throw error;});
  draftQueue=next;return next;
}
function updateHistoryButtons(){if(!active)return;for(const [id,enabled] of [['pptx-undo',active.history.canUndo],['pptx-redo',active.history.canRedo]] as const){const b=document.getElementById(id) as HTMLButtonElement|null;if(b){b.disabled=!enabled;b.dataset.unavailable=String(!enabled);}}}
function markChanged(){if(!active)return;const after={...active.committed,json:JSON.stringify(active.model)};if(!active.history.commit(active.committed,after))return;active.committed=after;active.dirty=after.json!==active.saved;active.revision++;queueDraft();updateHistoryButtons();}
function travelHistory(redo:boolean){
  const owner=active;if(!owner||paused||saving||leaving)return;
  const next=redo?owner.history.redo(owner.committed):owner.history.undo(owner.committed);if(!next)return;
  const focused=document.activeElement as HTMLElement|null;
  const selection=focused instanceof HTMLTextAreaElement?[focused.selectionStart,focused.selectionEnd]:null;
  owner.model=JSON.parse(next.json);owner.committed=next;owner.dirty=next.json!==owner.saved;owner.revision++;queueDraft();
  paint(Math.min(next.slide,owner.model.slides.length-1),next.box);
  const restored=focused?.id?document.getElementById(focused.id):null;restored?.focus();
  if(restored instanceof HTMLTextAreaElement&&selection)restored.setSelectionRange(...selection as [number,number]);
}
export function closePresentationEditor(){if(draftTimer)clearTimeout(draftTimer);draftTimer=null;active=null;}
export async function canLeavePresentation(){
  const owner=active;if(!owner)return true;
  if(saving||leaving||paused){showNotice('演示文稿操作尚未完成，请稍后再离开。');return false;}
  if(owner.dirty&&!window.confirm('演示文稿尚未另存 PPTX。保存本地草稿后离开？'))return false;
  leaving=true;const controls=document.querySelectorAll<HTMLInputElement|HTMLButtonElement|HTMLSelectElement>('#presentation input,#presentation button,#presentation select,#presentation textarea');controls.forEach(el=>el.disabled=true);
  try {
    const pending=draftTimer!==null;if(draftTimer)clearTimeout(draftTimer);draftTimer=null;
    if(owner.dirty||pending||owner.draftFailed||owner.revision!==owner.draftRevision)await writeDraft();else await draftQueue;
    return true;
  }catch(error){showNotice(`演示草稿保存失败，已保留当前文稿：${String(error)}`,true);return false;}
  finally{leaving=false;if(active===owner)controls.forEach(el=>el.disabled=el.dataset.unavailable==='true');}
}
export function newPresentationModel():PresentationModel{return {version:1,size_emu:[10972800,6172200],slides:[{text_boxes:[{bounds_emu:[914400,914400,7315200,1828800],text:'新演示文稿',font_size_hundredths_pt:2400,bold:true,italic:false,color_rgb:[39,94,168]}]}]};}
function view(model:PresentationModel):View{return {size_emu:model.size_emu,unreadable_parts:[],slides:model.slides.map((slide,i)=>({id:i+256,name:`幻灯片 ${i+1}`,hidden:slide.hidden??false,notes:slide.notes,text:slide.text_boxes.map(t=>t.text).join('\n'),parse_error:null,shapes:[...slide.text_boxes.map((t,j)=>({id:j+1,name:`文本框 ${j+1}`,kind:'shape',bounds_emu:t.bounds_emu,text:t.text,vertical_alignment:t.vertical_alignment,alignment:t.alignment==='center'?'ctr':t.alignment==='right'?'r':t.alignment==='left'?'l':undefined,runs:[{text:t.text,font_size_hundredths_pt:t.font_size_hundredths_pt,bold:t.bold,italic:t.italic,color_rgb:t.color_rgb}],children:[]})),...(slide.images??[]).map((p,j)=>({id:slide.text_boxes.length+j+1,name:p.alt||`图片 ${j+1}`,kind:'picture',bounds_emu:p.bounds_emu,text:'',runs:[],image:p.data_url,children:[]}))]}))};}
export function openPresentationEditor(model:PresentationModel,unsaved=false){active={model:structuredClone(model),dirty:unsaved,revision:0,draftRevision:unsaved?-1:0,draftFailed:false,history:new PresentationHistory(),committed:{json:JSON.stringify(model),slide:0,box:0},saved:unsaved?null:JSON.stringify(model)};paint(0,0);if(unsaved)queueDraft();}
function paint(slideIndex:number,boxIndex:number){
  const owner=active;if(!owner)return;const model=owner.model;
  boxIndex=Math.max(0,Math.min(boxIndex,model.slides[slideIndex].text_boxes.length-1));
  owner.committed={...owner.committed,slide:slideIndex,box:boxIndex};
  openPresentation(view(model),true,()=>view(owner.model));
  document.getElementById('stats')!.textContent=`${model.slides.length} 张幻灯片`;
  const host=document.getElementById('presentation')!;
  const picker=document.getElementById('slide-picker') as HTMLSelectElement;
  picker.value=String(slideIndex);picker.dispatchEvent(new Event('change'));
  picker.addEventListener('change',()=>paint(Number(picker.value),0));
  const panel=document.createElement('div');panel.id='presentation-editor';host.prepend(panel);
  const previewHost=document.getElementById('slide-preview')!;
  let selectedImage:number|null=null;
  const textCount=model.slides[slideIndex].text_boxes.length;
  const objectCount=textCount+(model.slides[slideIndex].images?.length??0);
  const objectAt=(index:number)=>index<textCount?model.slides[slideIndex].text_boxes[index]:model.slides[slideIndex].images![index-textCount];
  function highlightSelection(){
    previewHost.querySelector('svg')?.setAttribute('role','group');
    for(const group of previewHost.querySelectorAll<SVGGElement>('g[data-shape-id]')){
      const index=Number(group.dataset.shapeId)-1;if(index<0||index>=objectCount)continue;const selected=selectedImage===null?index===boxIndex:index===textCount+selectedImage;
      group.setAttribute('tabindex','0');group.setAttribute('role','button');group.setAttribute('aria-label',index<textCount?`选择文本框 ${index+1}`:`选择图片 ${index-textCount+1}`);group.setAttribute('aria-pressed',String(selected));group.style.cursor='move';group.style.touchAction='none';group.setAttribute('aria-description','方向键微调位置，Shift 加速；Enter 编辑；拖动右下角调整尺寸，Shift 保持当前框比例。');
      const rect=group.querySelector('rect');if(rect){rect.setAttribute('pointer-events','all');rect.setAttribute('stroke',selected?'#275ea8':'#8c938e');rect.setAttribute('stroke-width',selected?'2':'1');}
      group.querySelector('[data-resize-handle]')?.remove();
      if(selected){const [x,y,w,h]=objectAt(index).bounds_emu.map(n=>n/9525),size=Math.min(10,w,h),handle=document.createElementNS('http://www.w3.org/2000/svg','rect');
        for(const [key,value] of Object.entries({x:x+w-size,y:y+h-size,width:size,height:size,fill:'#275ea8','data-resize-handle':'se','aria-hidden':'true','pointer-events':'all'}))handle.setAttribute(key,String(value));
        handle.style.cursor='se-resize';group.append(handle);
      }
    }
  }
  function targetObject(event:Event){const group=event.target instanceof Element?event.target.closest<SVGGElement>('g[data-shape-id]'):null;
    if(!group||!previewHost.contains(group))return null;const index=Number(group.dataset.shapeId)-1;return Number.isInteger(index)&&index>=0&&index<objectCount?index:null;}
  let drag:{id:number,index:number,origin:[number,number,number,number],start:DOMPoint,matrix:DOMMatrix,client:[number,number],group:SVGGElement,x:number,y:number,w:number,h:number,resize:boolean,moved:boolean,revision:number}|null=null;
  let suppressClick=false;
  function cancelDrag(){const current=drag;drag=null;if(!current)return;current.group.removeAttribute('transform');current.group.querySelector('[data-resize-preview]')?.remove();if(current.group.hasPointerCapture(current.id))current.group.releasePointerCapture(current.id);}
  previewHost.onpointerdown=event=>{
    if(active!==owner||paused||saving||leaving||event.button!==0||!event.isPrimary)return;
    const index=targetObject(event),svg=previewHost.querySelector('svg');if(index===null||!svg)return;
    const matrix=svg.getScreenCTM();if(!matrix)return;const inverse=matrix.inverse(),start=new DOMPoint(event.clientX,event.clientY).matrixTransform(inverse);
    if(![start.x,start.y,inverse.a,inverse.d].every(Number.isFinite))return;
    const group=previewHost.querySelector<SVGGElement>(`g[data-shape-id="${index+1}"]`)!;
    const origin=[...objectAt(index).bounds_emu] as [number,number,number,number];
    cancelDrag();suppressClick=false;drag={id:event.pointerId,index,origin,start,matrix:inverse,client:[event.clientX,event.clientY],group,x:origin[0],y:origin[1],w:origin[2],h:origin[3],resize:event.target instanceof Element&&event.target.closest('[data-resize-handle]')!==null,moved:false,revision:owner.revision};
    group.focus();group.setPointerCapture(event.pointerId);event.preventDefault();
  };
  function moveDrag(event:PointerEvent){
    const current=drag;if(!current||current.id!==event.pointerId)return;
    if(!owner||active!==owner||owner.model!==model||owner.revision!==current.revision||paused||saving||leaving){cancelDrag();return;}
    if(Math.abs(event.clientX-current.client[0])+Math.abs(event.clientY-current.client[1])>=4)current.moved=true;
    if(!current.moved)return;
    const point=new DOMPoint(event.clientX,event.clientY).matrixTransform(current.matrix);
    if(current.resize){
      const [x,y,w,h]=current.origin,dx=(point.x-current.start.x)*9525,dy=(point.y-current.start.y)*9525;
      let width=w+dx,height=h+dy;
      if(event.shiftKey){const factor=Math.abs(dx/w)>=Math.abs(dy/h)?width/w:height/h,scale=Math.max(Math.max(1/w,1/h),Math.min(factor,(model.size_emu[0]-x)/w,(model.size_emu[1]-y)/h));width=w*scale;height=h*scale;}
      current.w=Math.max(1,Math.min(model.size_emu[0]-x,Math.round(width)));current.h=Math.max(1,Math.min(model.size_emu[1]-y,Math.round(height)));
      if(!Number.isSafeInteger(current.w)||!Number.isSafeInteger(current.h)){cancelDrag();return;}
      let outline=current.group.querySelector<SVGRectElement>('[data-resize-preview]');if(!outline){outline=document.createElementNS('http://www.w3.org/2000/svg','rect');outline.setAttribute('data-resize-preview','true');outline.setAttribute('fill','none');outline.setAttribute('stroke','#275ea8');outline.setAttribute('stroke-width','2');outline.setAttribute('stroke-dasharray','5 3');outline.setAttribute('pointer-events','none');outline.setAttribute('aria-hidden','true');current.group.append(outline);}
      for(const [key,value] of Object.entries({x:x/9525,y:y/9525,width:current.w/9525,height:current.h/9525}))outline.setAttribute(key,String(value));
      return;
    }
    current.x=Math.max(0,Math.min(model.size_emu[0]-current.origin[2],Math.round(current.origin[0]+(point.x-current.start.x)*9525)));
    current.y=Math.max(0,Math.min(model.size_emu[1]-current.origin[3],Math.round(current.origin[1]+(point.y-current.start.y)*9525)));
    if(!Number.isSafeInteger(current.x)||!Number.isSafeInteger(current.y)){cancelDrag();return;}
    current.group.setAttribute('transform',`translate(${(current.x-current.origin[0])/9525} ${(current.y-current.origin[1])/9525})`);
  }
  function commitMove(index:number,bounds:[number,number,number,number],focus=false,resize=false){
    if(!owner||active!==owner||owner.model!==model||paused||saving||leaving)return;
    objectAt(index).bounds_emu=bounds;
    owner.committed={...owner.committed,slide:slideIndex,box:index<textCount?index:boxIndex};markChanged();paint(slideIndex,index<textCount?index:boxIndex);
    if(index>=textCount){const picker=document.getElementById('pptx-image-picker') as HTMLSelectElement;picker.value=String(index-textCount);picker.dispatchEvent(new Event('change'));}
    if(focus)document.querySelector<SVGGElement>(`#slide-preview g[data-shape-id="${index+1}"]`)?.focus();
    showNotice(resize?'已调整对象尺寸，可整体撤销。':index<textCount?'已移动文本框，可整体撤销。':'已移动图片，可整体撤销。');
  }
  previewHost.onpointermove=moveDrag;
  previewHost.onpointerup=event=>{
    moveDrag(event);const current=drag;if(!current||current.id!==event.pointerId)return;
    cancelDrag();if(!current.moved)return;suppressClick=true;
    if(current.x===current.origin[0]&&current.y===current.origin[1]&&current.w===current.origin[2]&&current.h===current.origin[3])return;
    commitMove(current.index,[current.x,current.y,current.w,current.h],false,current.resize);
  };
  const cancelPointer=(event:PointerEvent)=>{if(drag?.id===event.pointerId)cancelDrag();};
  previewHost.onpointercancel=cancelPointer;previewHost.onlostpointercapture=cancelPointer;
  function selectObject(index:number,edit=false){
    if(index<textCount){if(index!==boxIndex||selectedImage!==null)paint(slideIndex,index);}
    else {selectedImage=index-textCount;const picker=document.getElementById('pptx-image-picker') as HTMLSelectElement;picker.value=String(selectedImage);picker.dispatchEvent(new Event('change'));highlightSelection();}
    if(edit){const input=document.getElementById(index<textCount?'pptx-box-text':'pptx-image-alt') as HTMLInputElement|HTMLTextAreaElement|null;input?.focus();input?.select();}
    else document.querySelector<SVGGElement>(`#slide-preview g[data-shape-id="${index+1}"]`)?.focus();
  }
  previewHost.onclick=event=>{if(suppressClick){suppressClick=false;return;}if(active!==owner||paused)return;const index=targetObject(event);if(index!==null)selectObject(index);};
  previewHost.ondblclick=event=>{if(active!==owner||paused)return;const index=targetObject(event);if(index!==null)selectObject(index,true);};
  previewHost.onkeydown=event=>{
    if(event.key==='Escape'&&drag){event.preventDefault();cancelDrag();return;}
    if(active!==owner||paused||saving||leaving||event.isComposing||event.ctrlKey||event.metaKey||event.altKey)return;
    const index=targetObject(event);if(index===null)return;
    if(['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)){
      event.preventDefault();cancelDrag();const [x,y,w,h]=objectAt(index).bounds_emu,step=9525*(event.shiftKey?10:1);
      const nextX=Math.max(0,Math.min(model.size_emu[0]-w,x+(event.key==='ArrowRight'?step:event.key==='ArrowLeft'?-step:0)));
      const nextY=Math.max(0,Math.min(model.size_emu[1]-h,y+(event.key==='ArrowDown'?step:event.key==='ArrowUp'?-step:0)));
      if(nextX===x&&nextY===y){selectObject(index);return;}
      commitMove(index,[nextX,nextY,w,h],true);return;
    }
    if(!['Enter',' '].includes(event.key))return;event.preventDefault();selectObject(index,event.key==='Enter');
  };
  highlightSelection();
  panel.addEventListener('keydown',event=>{if((event.ctrlKey||event.metaKey)&&!event.altKey&&['z','y'].includes(event.key.toLowerCase())){event.preventDefault();event.stopPropagation();travelHistory(event.key.toLowerCase()==='y'||event.shiftKey);}});
  const state=document.createElement('p');state.id='presentation-edit-state';panel.append(state);
  const changed=()=>{markChanged();state.textContent=owner.dirty?'有未另存的修改 · 正在等待草稿保存':'与已另存 PPTX 一致 · 正在等待草稿保存';
    const report=view(model);const preview=document.getElementById('slide-preview')!;preview.replaceChildren();renderPreview(preview,document.getElementById('slide-preview-info')!,report.size_emu,report.slides[slideIndex].shapes,null);
    document.getElementById('slide-text')!.textContent=report.slides[slideIndex].text;highlightSelection();
  };
  state.textContent=owner.dirty?'有未另存的修改 · 本地草稿自动保存':'已载入自建编辑模型';
  const button=(id:string,label:string,click:()=>void)=>{const b=document.createElement('button');b.id=id;b.textContent=label;b.addEventListener('click',click);panel.append(b);return b;};
  button('pptx-undo','撤销',()=>travelHistory(false));button('pptx-redo','重做',()=>travelHistory(true));updateHistoryButtons();
  const sizePicker=document.createElement('select');sizePicker.id='pptx-page-size';sizePicker.setAttribute('aria-label','演示页面尺寸');
  for(const [value,label] of [['current',`当前 ${model.size_emu.map(n=>(n/914400).toFixed(2)).join(' × ')} 英寸`],['wide','16:9（12 × 6.75 英寸）'],['standard','4:3（10 × 7.5 英寸）']]){const option=document.createElement('option');option.value=value;option.textContent=label;sizePicker.append(option);}panel.append(sizePicker);
  button('pptx-apply-page-size','应用页面尺寸',()=>{if(sizePicker.value==='current')return;void operation('正在调整演示页面尺寸…',async()=>{
    const candidate=resizePresentation(model,sizePicker.value==='wide'?[10972800,6172200]:[9144000,6858000]);
    await invoke('validate_presentation',{model:candidate});if(active!==owner)return;
    owner.model=candidate;markChanged();paint(slideIndex,boxIndex);showNotice('已按统一比例居中调整页面内容与字号，可撤销；图片未拉伸。');
  });});
  const sizeNote=document.createElement('p');sizeNote.textContent='切换尺寸会等比缩放并居中所有页的文字和图片，可能留白；请检查换行和文字溢出。';panel.append(sizeNote);
  button('pptx-add-slide','新增幻灯片',()=>{if(model.slides.length>=100){showNotice('最多 100 页',true);return;}model.slides.push({text_boxes:[]});markChanged();paint(model.slides.length-1,0);});
  button('pptx-toggle-hidden',model.slides[slideIndex].hidden?'取消隐藏当前页':'隐藏当前页',()=>{const slide=model.slides[slideIndex];if(slide.hidden)delete slide.hidden;else slide.hidden=true;markChanged();paint(slideIndex,boxIndex);});
  const duplicate=button('pptx-copy-slide','复制当前页',()=>void operation('正在复制幻灯片…',async()=>{
    const candidate=structuredClone(model);candidate.slides.splice(slideIndex+1,0,structuredClone(model.slides[slideIndex]));
    await invoke('validate_presentation',{model:candidate});if(active!==owner)return;
    owner.model=candidate;markChanged();paint(slideIndex+1,boxIndex);showNotice('已复制当前页，可继续编辑并另存 PPTX。');
  }));duplicate.disabled=model.slides.length>=100;duplicate.dataset.unavailable=String(duplicate.disabled);
  for(const [id,label,offset] of [['pptx-slide-up','当前页前移',-1],['pptx-slide-down','当前页后移',1]] as const){
    const target=slideIndex+offset;const move=button(id,label,()=>{const [slide]=model.slides.splice(slideIndex,1);model.slides.splice(target,0,slide);markChanged();paint(target,boxIndex);});
    move.disabled=target<0||target>=model.slides.length;move.dataset.unavailable=String(move.disabled);
  }
  const remove=button('pptx-delete-slide','删除当前页',()=>{model.slides.splice(slideIndex,1);markChanged();paint(Math.min(slideIndex,model.slides.length-1),0);});remove.disabled=model.slides.length<=1;remove.dataset.unavailable=String(remove.disabled);
  button('pptx-add-box','新增文本框',()=>{if(model.slides.reduce((n,s)=>n+s.text_boxes.length+(s.images?.length??0),0)>=10000){showNotice('文本框数量超限',true);return;}model.slides[slideIndex].text_boxes.push({...newPresentationModel().slides[0].text_boxes[0],text:'新文本框'});markChanged();paint(slideIndex,model.slides[slideIndex].text_boxes.length-1);});
  button('pptx-editor-save','另存 PPTX',()=>void operation('正在另存演示文稿…',async()=>{
    saving=true;try {
    const saved=await invoke<{path:string}|null>('save_new_presentation',{model:structuredClone(model)});
    if(!saved){showNotice('已取消另存，修改仍保留。');return;}
    if(active===owner){owner.saved=JSON.stringify(model);owner.dirty=false;state.textContent='PPTX 已另存';document.getElementById('subtitle')!.textContent=saved.path;showNotice(`已另存 PPTX：${saved.path}`);}
    } finally {saving=false;}
  }));
  paintPictures(panel,model,slideIndex,changed,()=>paint(slideIndex,boxIndex));
  document.getElementById('pptx-image-picker')?.addEventListener('change',event=>{selectedImage=Number((event.target as HTMLSelectElement).value);highlightSelection();});
  const notesLabel=document.createElement('label');notesLabel.textContent='演讲者备注（不显示在放映或 PDF 中）';
  const notesInput=document.createElement('textarea');notesInput.id='pptx-edit-notes';notesInput.setAttribute('aria-label','编辑演讲者备注');notesInput.value=model.slides[slideIndex].notes??'';notesInput.style.cssText='display:block;width:100%;min-height:80px';notesLabel.append(notesInput);panel.append(notesLabel);
  notesInput.addEventListener('input',()=>{
    const slide=model.slides[slideIndex],value=notesInput.value;
    const total=model.slides.reduce((n,s)=>n+new TextEncoder().encode(s===slide?value:s.notes??'').length+s.text_boxes.reduce((n,t)=>n+new TextEncoder().encode(t.text).length,0),0);
    if(total>1024*1024 || value.split('\n').length>1000 || /[\u0000-\u001F\u007F-\u009F]/.test(value.replaceAll('\n',''))){notesInput.value=slide.notes??'';showNotice('备注包含未支持控制字符或超过编辑上限',true);return;}
    if(value)slide.notes=value;else delete slide.notes;
    markChanged();const notes=document.getElementById('slide-notes');if(notes){notes.textContent=value;notes.parentElement!.hidden=!value;}
  });
  const boxes=model.slides[slideIndex].text_boxes;
  const boxPicker=document.createElement('select');boxPicker.id='pptx-box-picker';boxPicker.setAttribute('aria-label','编辑文本框');
  boxes.forEach((_,i)=>{const option=document.createElement('option');option.value=String(i);option.textContent=`文本框 ${i+1}`;boxPicker.append(option);});panel.append(boxPicker);boxPicker.value=String(boxIndex);boxPicker.addEventListener('change',()=>paint(slideIndex,Number(boxPicker.value)));
  const box=boxes[boxIndex];if(!box){boxPicker.disabled=true;boxPicker.dataset.unavailable='true';return;}
  const align=document.createElement('select');align.id='pptx-box-alignment';align.setAttribute('aria-label','文本框对齐');
  for(const [value,label] of [['left','左对齐'],['center','居中'],['right','右对齐']]){const option=document.createElement('option');option.value=value;option.textContent=label;align.append(option);}
  align.value=box.alignment??'left';panel.append(align);
  const vertical=document.createElement('select');vertical.id='pptx-box-vertical';vertical.setAttribute('aria-label','文本框垂直对齐');
  for(const [value,label] of [['top','顶部'],['middle','垂直居中'],['bottom','底部']]){const option=document.createElement('option');option.value=value;option.textContent=label;vertical.append(option);}
  vertical.value=box.vertical_alignment??'top';panel.append(vertical);
  vertical.addEventListener('change',()=>{if(vertical.value==='top')delete box.vertical_alignment;else box.vertical_alignment=vertical.value as 'middle'|'bottom';changed();});
  align.addEventListener('change',()=>{if(align.value==='left')delete box.alignment;else box.alignment=align.value as 'center'|'right';changed();});
  for(const [id,label,offset] of [['pptx-box-back','文本框后移一层',-1],['pptx-box-front','文本框前移一层',1]] as const){
    const target=boxIndex+offset;
    const move=button(id,label,()=>{const [selected]=boxes.splice(boxIndex,1);boxes.splice(target,0,selected);markChanged();paint(slideIndex,target);});
    move.disabled=target<0||target>=boxes.length;move.dataset.unavailable=String(move.disabled);
  }
  const layerNote=document.createElement('p');layerNote.textContent='层次调整仅作用于当前页的文本框；图片始终绘制在文本框之后。';panel.append(layerNote);
  button('pptx-copy-box','复制文本框',()=>void operation('正在复制文本框…',async()=>{
    const candidate=structuredClone(model);
    candidate.slides[slideIndex].text_boxes.splice(boxIndex+1,0,structuredClone(box));
    await invoke('validate_presentation',{model:candidate});if(active!==owner)return;
    owner.model=candidate;markChanged();paint(slideIndex,boxIndex+1);
    showNotice('已复制文本框并选中副本；位置与原框相同，可在位置设置中移动。');
  }));
  button('pptx-delete-box','删除文本框',()=>{boxes.splice(boxIndex,1);markChanged();paint(slideIndex,0);});
  const input=document.createElement('textarea');input.id='pptx-box-text';input.setAttribute('aria-label','文本框文字');input.value=box.text;input.style.cssText='display:block;width:100%;min-height:100px;margin:12px 0';panel.append(input);
  input.addEventListener('input',()=>{
    if(model.slides.reduce((total,s)=>total+new TextEncoder().encode(s.notes??'').length+s.text_boxes.reduce((n,t)=>n+new TextEncoder().encode(t===box?input.value:t.text).length,0),0)>1024*1024 || input.value.split('\n').length>1000 || /[\u0000-\u0009\u000B-\u001F]/.test(input.value)){input.value=box.text;showNotice('文字包含未支持控制字符或超过编辑上限',true);return;}
    box.text=input.value;changed();
  });
  for(const key of ['bold','italic'] as const){const label=document.createElement('label');const check=document.createElement('input');check.type='checkbox';check.id=`pptx-box-${key}`;check.checked=box[key];check.addEventListener('change',()=>{box[key]=check.checked;changed();});label.append(check,key==='bold'?'粗体':'斜体');panel.append(label);}
  const size=document.createElement('input');size.id='pptx-box-size';size.type='number';size.min='1';size.max='400';size.step='.01';size.setAttribute('aria-label','字号 pt');size.value=String(box.font_size_hundredths_pt/100);panel.append(document.createTextNode("字号 pt "),size);size.addEventListener('change',()=>{const n=Number(size.value);if(!Number.isFinite(n)||n<1||n>400){size.value=String(box.font_size_hundredths_pt/100);return;}box.font_size_hundredths_pt=Math.round(n*100);changed();});
  const color=document.createElement('input');color.type='color';color.id='pptx-box-color';color.setAttribute('aria-label','文字颜色');color.value='#'+box.color_rgb.map(n=>n.toString(16).padStart(2,'0')).join('');panel.append(document.createTextNode("文字颜色 "),color);color.addEventListener('input',()=>{box.color_rgb=[1,3,5].map(i=>parseInt(color.value.slice(i,i+2),16)) as [number,number,number];changed();});
  const bounds=box.bounds_emu.map((value,i)=>{const label=document.createElement('label');label.textContent=['左 cm','上 cm','宽 cm','高 cm'][i];const field=document.createElement('input');field.type='number';field.step='.1';field.id=`pptx-box-bound-${i}`;field.value=String(value/360000);field.style.width='6em';label.append(field);panel.append(label);return field;});
  button('pptx-box-position','应用位置',()=>{const values=bounds.map(field=>Math.round(Number(field.value)*360000));const [x,y,w,h]=values;
    if(!values.every(Number.isSafeInteger)||bounds.some(f=>f.value==='')||x<0||y<0||w<=0||h<=0||x+w>model.size_emu[0]||y+h>model.size_emu[1]){showNotice('文本框位置必须完全位于幻灯片内',true);return;}
    box.bounds_emu=values as [number,number,number,number];changed();
  });
}

function paintPictures(panel:HTMLElement,model:PresentationModel,slideIndex:number,changed:()=>void,repaint:()=>void){
  const owner=active,slide=model.slides[slideIndex];
  const heading=document.createElement('p');heading.textContent='图片 · PNG/JPEG，单张最多 1 MiB，整份最多 2 MiB';panel.append(heading);
  const file=document.createElement('input');file.id='pptx-image-file';file.type='file';file.accept='image/png,image/jpeg';file.setAttribute('aria-label','插入演示图片');panel.append(file);
  file.addEventListener('change',()=>{const chosen=file.files?.[0];file.value='';if(!chosen)return;
    if(chosen.size>1024*1024||!['image/png','image/jpeg'].includes(chosen.type)){showNotice('请选择不超过 1 MiB 的 PNG/JPEG 图片',true);return;}
    void operation('正在检查演示图片…',async()=>{
      const data=await new Promise<string>((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(String(reader.result));reader.onerror=()=>reject(new Error('图片读取失败'));reader.readAsDataURL(chosen);});
      const [width,height]=await invoke<[number,number]>('inspect_presentation_picture',{dataUrl:data});
      if(active!==owner)return;
      if(!Number.isSafeInteger(width)||!Number.isSafeInteger(height)||width<=0||height<=0)throw new Error('图片尺寸无效');
      const count=model.slides.reduce((n,s)=>n+s.text_boxes.length+(s.images?.length??0),0);
      const bytes=model.slides.reduce((n,s)=>n+(s.images??[]).reduce((m,p)=>m+atob(p.data_url.split(',')[1]).length,0),0);
      if(count>=10000||bytes+chosen.size>2*1024*1024)throw new Error('图片或对象总量超过编辑上限');
      const w=Math.min(model.size_emu[0]/2,model.size_emu[1]/2*width/height),h=w*height/width;
      const picture:Picture={bounds_emu:[0,0,Math.max(1,Math.round(w)),Math.max(1,Math.round(h))],data_url:data,alt:''};
      const candidate=structuredClone(model);(candidate.slides[slideIndex].images??=[]).push(picture);
      if(new TextEncoder().encode(JSON.stringify(candidate)).length>4*1024*1024)throw new Error('演示模型超过 4 MiB');
      await invoke('validate_presentation',{model:candidate});
      if(active!==owner)return;
      (slide.images??=[]).push(picture);markChanged();repaint();
    });
  });
  const pictures=slide.images??[];if(!pictures.length)return;
  const picker=document.createElement('select');picker.id='pptx-image-picker';picker.setAttribute('aria-label','编辑图片');pictures.forEach((_,i)=>{const option=document.createElement('option');option.value=String(i);option.textContent=`图片 ${i+1}`;picker.append(option);});panel.append(picker);
  const fields=document.createElement('div');panel.append(fields);
  const paint=()=>{fields.replaceChildren();const index=Number(picker.value),picture=pictures[index];
    const alt=document.createElement('input');alt.id='pptx-image-alt';alt.value=picture.alt;alt.maxLength=256;alt.setAttribute('aria-label','图片替代文字');fields.append(alt);alt.addEventListener('change',()=>{if(/[\u0000-\u001f\u007f-\u009f]/.test(alt.value)){alt.value=picture.alt;return;}picture.alt=alt.value;changed();});
    const coords=picture.bounds_emu.map((n,i)=>{const label=document.createElement('label');label.textContent=['左 cm','上 cm','宽 cm','高 cm'][i];const input=document.createElement('input');input.id=`pptx-image-bound-${i}`;input.type='number';input.step='.1';input.value=String(n/360000);input.style.width='6em';label.append(input);fields.append(label);return input;});
    const apply=document.createElement('button');apply.id='pptx-image-position';apply.textContent='应用图片位置';fields.append(apply);apply.addEventListener('click',()=>{const values=coords.map(i=>Math.round(Number(i.value)*360000));const [x,y,w,h]=values;if(coords.some(i=>i.value==='')||!values.every(Number.isSafeInteger)||x<0||y<0||w<=0||h<=0||x+w>model.size_emu[0]||y+h>model.size_emu[1]){showNotice('图片位置必须完全位于幻灯片内',true);return;}picture.bounds_emu=values as [number,number,number,number];changed();});
    const restore=document.createElement('button');restore.id='pptx-image-restore-ratio';restore.textContent='恢复原始比例';fields.append(restore);
    restore.addEventListener('click',()=>{if(!owner||active!==owner||paused||saving||leaving)return;const revision=owner.revision;
      void operation('正在恢复图片比例…',async()=>{
        const [width,height]=await invoke<[number,number]>('inspect_presentation_picture',{dataUrl:picture.data_url});
        if(active!==owner||owner.model!==model||owner.revision!==revision)return;
        if(!Number.isSafeInteger(width)||!Number.isSafeInteger(height)||width<=0||height<=0)throw new Error('图片尺寸无效');
        const [x,y,w,h]=picture.bounds_emu,scale=Math.min(w/width,h/height);
        const bounds:[number,number,number,number]=[x,y,Math.min(w,Math.max(1,Math.round(width*scale))),Math.min(h,Math.max(1,Math.round(height*scale)))];
        const candidate=structuredClone(model);candidate.slides[slideIndex].images![index].bounds_emu=bounds;
        await invoke('validate_presentation',{model:candidate});
        if(active!==owner||owner.model!==model||owner.revision!==revision)return;
        picture.bounds_emu=bounds;changed();paint();showNotice('已在当前图片框内恢复原始比例，可撤销。');
      });
    });
    const remove=document.createElement('button');remove.id='pptx-image-delete';remove.textContent='删除图片';fields.append(remove);remove.addEventListener('click',()=>{pictures.splice(index,1);if(!pictures.length)delete slide.images;markChanged();repaint();});
  };picker.addEventListener('change',paint);paint();
}
