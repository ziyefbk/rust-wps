import {test,expect} from '@playwright/test';
import {Schema} from '@tiptap/pm/model';
import {loadTsModule} from './load-ts.mjs';
import {nativeBridge} from './bridge.mjs';
const {searchWord}=await loadTsModule('ui/word-search.ts');
const schema=new Schema({nodes:{doc:{content:'paragraph+'},paragraph:{content:'inline*'},text:{group:'inline'},hardBreak:{inline:true,group:'inline',atom:true},image:{inline:true,group:'inline',atom:true}},marks:{bold:{}}});
const paragraph=content=>schema.node('paragraph',null,content),text=value=>schema.text(value);

test('replace current match preserves surrounding styles, uses first marks, deletes and undoes once',async({page})=>{
 page.on('dialog',dialog=>dialog.accept());
 const source={type:'doc',content:[{type:'paragraph',content:[{type:'text',text:'前中',marks:[{type:'bold'}]},{type:'text',text:'文后',marks:[{type:'italic'}]}]}]};
 let draft,writes=0;
 await nativeBridge(page,async(command,args)=>{if(command==='load_draft')return source;if(command==='save_draft'){draft=args.doc;writes++;return;}throw new Error(command);});
 await page.goto('/');await page.locator('#restore-draft').click();
 await page.locator('#edit-find').fill('中文');await page.locator('#edit-replacement').fill('替代😀');
 await expect(page.locator('#edit-replace-current')).toBeDisabled();await page.locator('#edit-find-next').click();await page.locator('#edit-replace-current').click();
 await expect(page.locator('.tiptap p')).toHaveText('前替代😀后');await expect(page.locator('.tiptap strong')).toHaveText('前替代😀');await expect(page.locator('.tiptap em')).toHaveText('后');
 await expect.poll(()=>draft?.content[0]?.content[0]?.text).toBe('前替代😀');await expect(page.locator('#edit-replace-current')).toBeDisabled();
 await page.locator('#edit-undo').click();await expect(page.locator('.tiptap strong')).toHaveText('前中');await expect(page.locator('.tiptap em')).toHaveText('文后');
 await expect.poll(()=>draft?.content[0]?.content[0]?.text).toBe('前中');const before=writes;
 await page.locator('#edit-find-next').click();await page.locator('#edit-replacement').fill('中文');await page.locator('#edit-replace-current').click();await page.waitForTimeout(800);expect(writes).toBe(before);
 await page.locator('#edit-replacement').fill('');await page.locator('#edit-replace-current').click();await expect(page.locator('.tiptap p')).toHaveText('前后');
 await page.locator('#edit-undo').click();await expect(page.locator('.tiptap p')).toHaveText('前中文后');
 await page.locator('#edit-find-next').click();
 await expect.poll(()=>writes).toBeGreaterThan(before);await expect.poll(()=>draft?.content[0]?.content[0]?.text).toBe('前中');const writesBeforeInvalid=writes;
 for(const value of ['bad\u0001text','bad\ufffe','bad\uffff','bad\ud800','x'.repeat(4097)]){
  await page.locator('#edit-replacement').evaluate((input,value)=>{input.value=value;},value);await page.locator('#edit-replace-current').click();await expect(page.getByRole('status')).toContainText('不支持换行');await expect(page.locator('.tiptap p')).toHaveText('前中文后');
 }
 await page.waitForTimeout(800);expect(writes).toBe(writesBeforeInvalid);await expect(page.locator('#edit-undo')).toBeDisabled();await expect(page.locator('#edit-redo')).toBeEnabled();
 await page.locator('#new-document').click();await expect(page.locator('#edit-replacement')).toHaveValue('');
});

test('replacement keeps first-character link and separates adjacent typing history',async({page})=>{
 const source={type:'doc',content:[{type:'paragraph',content:[{type:'text',text:'中文',marks:[{type:'link',attrs:{href:'https://example.com/'}}]}]}]};
 await nativeBridge(page,async(command)=>{if(command==='load_draft')return source;if(command==='save_draft')return;throw new Error(command);});
 await page.goto('/');await page.locator('#restore-draft').click();await page.locator('.tiptap').click();await page.keyboard.press('Home');await page.keyboard.insertText('前');
 await page.locator('#edit-find').fill('中文');await page.locator('#edit-find-next').click();await page.locator('#edit-replacement').fill('替换');await page.locator('#edit-replace-current').click();
 await expect(page.locator('.tiptap a')).toContainText('替换');await expect(page.locator('.tiptap a')).toHaveAttribute('href','https://example.com/');
 await page.keyboard.insertText('后');await expect(page.locator('.tiptap p')).toHaveText('前替换后');
 await page.locator('#edit-undo').click();await expect(page.locator('.tiptap p')).toHaveText('前替换');await page.locator('#edit-undo').click();await expect(page.locator('.tiptap p')).toHaveText('前中文');await page.locator('#edit-undo').click();await expect(page.locator('.tiptap p')).toHaveText('中文');
});

test('word search spans character styles but keeps structural boundaries and budgets',()=>{
 const doc=schema.node('doc',null,[paragraph([text('中'),schema.text('文',[schema.mark('bold')])]),paragraph([text('中'),schema.node('hardBreak'),text('文')]),paragraph([text('中'),schema.node('image'),text('文')]),paragraph([text('Aa 😀')])]);
 expect(searchWord(doc,'中文')).toEqual({matches:[{from:1,to:3}],truncated:false});expect(searchWord(doc,'a').matches).toHaveLength(1);expect(searchWord(doc,'文中').matches).toHaveLength(0);
 const emoji=searchWord(doc,'😀').matches[0];expect(doc.textBetween(emoji.from,emoji.to)).toBe('😀');expect(searchWord(doc,'\ud83d').truncated).toBe(true);
 expect(searchWord(schema.node('doc',null,Array.from({length:100001},()=>paragraph([]))),'missing')).toEqual({matches:[],truncated:true});
 expect(searchWord(doc,'x'.repeat(257))).toEqual({matches:[],truncated:true});expect(searchWord(doc,'').matches).toHaveLength(0);
 const many=searchWord(schema.node('doc',null,[paragraph([text('x'.repeat(1001))])]),'x');expect(many.matches).toHaveLength(1000);expect(many.truncated).toBe(true);
 expect(searchWord(schema.node('doc',null,[paragraph([text('x'.repeat(2*1024*1024+1))])]),'x')).toEqual({matches:[],truncated:true});
});

test('editor find navigates styled Chinese and table text without drafts, refreshes after editing and undo',async({page})=>{
 const source={type:'doc',content:[{type:'heading',attrs:{level:2},content:[{type:'text',text:'中',marks:[{type:'bold'}]},{type:'text',text:'文',marks:[{type:'italic'}]}]},{type:'paragraph',content:[{type:'text',text:'中文'}]},{type:'table',content:[{type:'tableRow',content:[{type:'tableCell',content:[{type:'paragraph',content:[{type:'text',text:'中文'}]}]}]}]},{type:'paragraph',content:[{type:'text',text:'中'},{type:'hardBreak'},{type:'text',text:'文'}]}]};
 let writes=0,draft;
 await nativeBridge(page,async(command,args)=>{if(command==='load_draft')return source;if(command==='save_draft'){writes++;draft=args.doc;return;}throw new Error(command);});
 await page.goto('/');await page.locator('#restore-draft').click();await expect(page.locator('.tiptap h2')).toHaveText('中文');await page.keyboard.press('Control+f');await expect(page.locator('#edit-find')).toBeFocused();await page.locator('#edit-find').fill('中文');await expect(page.locator('#edit-find-state')).toHaveText('共3 处');
 await page.locator('#edit-find').press('Enter');expect(await page.evaluate(()=>window.getSelection().toString())).toBe('中文');await expect(page.locator('#edit-find-state')).toHaveText('1/3 处');await page.waitForTimeout(800);expect(writes).toBe(0);
 await page.locator('#edit-find-next').click();await expect(page.locator('#edit-find-state')).toHaveText('2/3 处');await page.locator('#edit-find').press('Shift+Enter');await expect(page.locator('#edit-find-state')).toHaveText('1/3 处');
 await page.keyboard.insertText('替换');await expect(page.locator('.tiptap h2')).toHaveText('替换');await expect(page.locator('#edit-find-state')).toHaveText('共2 处');await expect.poll(()=>draft?.content[0]?.content.map(n=>n.text).join('')).toBe('替换');
 await page.locator('#edit-undo').click();await expect(page.locator('.tiptap h2')).toHaveText('中文');await expect(page.locator('#edit-find-state')).toHaveText('共3 处');
 await page.locator('#edit-find-prev').click();await expect(page.locator('#edit-find-state')).toHaveText('3/3 处');expect(await page.evaluate(()=>window.getSelection().anchorNode.parentElement.closest('td')!==null)).toBe(true);
 await page.locator('#edit-find').press('Escape');await expect(page.locator('#edit-find')).toHaveValue('');await expect(page.locator('#edit-find-next')).toBeDisabled();await expect(page.locator('.tiptap')).toBeFocused();
 await page.locator('#new-document').click();await expect(page.locator('#edit-find-state')).toHaveText('');await expect(page.locator('#edit-find-prev')).toBeDisabled();
});
