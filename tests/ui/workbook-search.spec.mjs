import {test,expect} from '@playwright/test';
import {loadTsModule} from './load-ts.mjs';
import {nativeBridge} from './bridge.mjs';
const {searchWorkbook}=await loadTsModule('ui/workbook-search.ts');
const cell=(row,column,value,formula=null)=>({row,column,address:'',value,formula,formula_cache:null,number_format:null});
const sheet=(name,cells,visibility='Visible')=>({name,cells,visibility,kind:'Worksheet',merged_ranges:[]});
const workbook=sheets=>({sheets,date1904:false,recalculated:false,warnings:[]});

test('literal workbook search distinguishes raw values and formulas and bounds sparse traversal',()=>{
 const source=workbook([sheet('Data',[cell(0,0,{type:'text',value:'中文 <x> 中文'}),cell(1,1,{type:'number',value:123},'SUM(A1,123)')]),sheet('Hidden',[cell(9,2,{type:'boolean',value:true})],'Hidden')]);
 expect(searchWorkbook(source,'中文').matches).toEqual([{sheet:0,row:0,column:0,field:'value'}]);
 expect(searchWorkbook(source,'123').matches.map(match=>match.field)).toEqual(['value','formula']);
 expect(searchWorkbook(source,'=sum').matches).toEqual([{sheet:0,row:1,column:1,field:'formula'}]);
 expect(searchWorkbook(source,'TRUE').matches).toEqual([{sheet:1,row:9,column:2,field:'value'}]);
 expect(searchWorkbook(source,'<x>').matches).toHaveLength(1);expect(searchWorkbook(source,'').matches).toHaveLength(0);
 expect(searchWorkbook(source,'x'.repeat(257)).truncated).toBe(true);
 expect(searchWorkbook(workbook([sheet('Budget',[cell(0,0,{type:'text',value:'x'.repeat(2*1024*1024+1)})])]),'x')).toEqual({matches:[],truncated:true});
 const many=searchWorkbook(workbook([sheet('Many',Array.from({length:1001},(_,row)=>cell(row,0,{type:'text',value:'hit'})))]),'hit');expect(many.truncated).toBe(true);expect(many.matches).toHaveLength(1000);
 expect(searchWorkbook(workbook([sheet('Sparse',Array.from({length:100001},(_,row)=>cell(row,0,{type:'empty'})))]),'missing')).toEqual({matches:[],truncated:true});
});

test('read-only search jumps to hidden far cells and formula source without changing workbook content',async({page})=>{
 const source=workbook([sheet('Data',[cell(0,0,{type:'text',value:'中文'}),cell(0,1,{type:'empty'},'SUM(A1:A2)')]),sheet('Hidden',[cell(999,25,{type:'text',value:'中文 <script>'})],'Hidden')]);
 await nativeBridge(page,async command=>{if(command==='open_document')return {name:'search.xlsx',path:'search.xlsx',inspection:{format:'xlsx',text:'',bytes:1,warnings:[]},workbook:source};throw new Error(command);});
 await page.goto('/');await page.locator('#open').click();await page.locator('#sheet-viewport').focus();await page.keyboard.press('Control+f');await expect(page.locator('#sheet-search')).toBeFocused();
 await page.locator('#sheet-search').fill('中文');await page.locator('#sheet-search').press('Enter');await expect(page.locator('#sheet-address')).toHaveValue('A1');
 await page.locator('#sheet-search').press('Enter');await expect(page.locator('#sheet-picker')).toHaveValue('1');await expect(page.locator('#sheet-address')).toHaveValue('Z1000');await expect(page.locator('#sheet-formula')).toHaveValue('中文 <script>');
 await expect(page.locator('#sheet-search-state')).toContainText('隐藏');await page.locator('#sheet-search').press('Shift+Enter');await expect(page.locator('#sheet-address')).toHaveValue('A1');
 await page.locator('#sheet-search').fill('=sum');await page.locator('#sheet-search-next').click();await expect(page.locator('#sheet-address')).toHaveValue('B1');await expect(page.locator('#sheet-formula')).toHaveValue('=SUM(A1:A2)');await expect(page.locator('#sheet-search-state')).toContainText('公式');
 await page.locator('#new-document').click();await expect(page.locator('#sheet-search')).toHaveValue('');
});

test('editing and undo refresh search hits rather than navigating stale cells',async({page})=>{
 const model={version:1,sheets:[{name:'Data',cells:[{row:0,column:0,content:{type:'text',value:'target'}}]}]};
 await nativeBridge(page,async command=>{if(command==='load_workbook_draft')return model;if(command==='save_workbook_draft')return;throw new Error(command);});
 await page.goto('/');await page.locator('#restore-workbook').click();await page.locator('#sheet-search').fill('target');await page.locator('#sheet-search-next').click();
 await page.locator('#sheet-formula').fill('changed');await page.locator('#sheet-apply').click();await expect(page.locator('#sheet-search-state')).toContainText('0 个匹配');await expect(page.locator('#sheet-search-next')).toBeDisabled();
 await page.locator('#sheet-undo').click();await expect(page.locator('#sheet-search-state')).toContainText('1 个匹配');await page.locator('#sheet-search-next').click();await expect(page.locator('#sheet-formula')).toHaveValue('target');
});
