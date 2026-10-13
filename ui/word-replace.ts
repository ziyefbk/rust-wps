import type {Node} from '@tiptap/pm/model';
import type {EditorState,Transaction} from '@tiptap/pm/state';
import type {WordMatch} from './word-search';

const encoder=new TextEncoder();
function checkBudget(doc:Node,enforce=true):number{
 let nodes=0,bytes=0;
 const stack=[{node:doc,depth:0}];
 while(stack.length){
  const {node,depth}=stack.pop()!;
  if(++nodes>(enforce?10000:100000)||depth>32)throw new Error('文档结构超出替换或保存上限，未修改正文。');
  if(node.isText){bytes+=encoder.encode(node.text!).length;if(enforce&&bytes>2*1024*1024)throw new Error('替换后正文超过 2 MiB 保存上限，未修改正文。');}
  for(let i=node.childCount-1;i>=0;i--)stack.push({node:node.child(i),depth:depth+1});
 }
 return bytes;
}

// Build and validate the entire change before dispatch; never partly replace.
export function createWordReplacement(state:EditorState,matches:WordMatch[],query:string,replacement:string):Transaction|null{
 if(replacement.length>4096||/[\u0000-\u001f\u007f\ufffe\uffff]|[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/u.test(replacement))
  throw new Error('替换文字限 4096 字符，不支持换行、控制字符或 DOCX 无法保存的 Unicode 字符。');
 if(!matches.length||!query||replacement===query)return null;
 if(matches.length>1000)throw new Error('匹配数量超出替换上限，未修改正文。');
 let end=-1;
 for(const match of matches){
  if(!Number.isInteger(match.from)||!Number.isInteger(match.to)||match.from<end||match.from<0||match.to<=match.from||match.to>state.doc.content.size||state.doc.textBetween(match.from,match.to)!==query)
   throw new Error('查找结果已失效，请重新查找。');
  const start=state.doc.resolve(match.from),finish=state.doc.resolve(match.to);
  if(!start.sameParent(finish)||!start.parent.isTextblock||match.to-match.from!==query.length)throw new Error('替换范围跨越结构边界，未修改正文。');
  end=match.to;
 }
 const bytes=checkBudget(state.doc,false)+(encoder.encode(replacement).length-encoder.encode(query).length)*matches.length;
 if(bytes>2*1024*1024)throw new Error('替换后正文超过 2 MiB 保存上限，未修改正文。');
 const tr=state.tr;
 for(let i=matches.length-1;i>=0;i--){
  const {from,to}=matches[i];
  if(replacement)tr.replaceWith(from,to,state.schema.text(replacement,state.doc.nodeAt(from)?.marks));
  else tr.delete(from,to);
 }
 checkBudget(tr.doc);
 if(encoder.encode(JSON.stringify({version:1,document:tr.doc.toJSON()})).length>32*1024*1024)throw new Error('替换后草稿超过 32 MiB 保存上限，未修改正文。');
 return tr;
}
