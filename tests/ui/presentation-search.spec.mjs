import {test,expect} from '@playwright/test';
import {loadTsModule} from './load-ts.mjs';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
const {searchPresentation}=await loadTsModule('ui/presentation-search.ts');
const report=slides=>({size_emu:null,unreadable_parts:[],slides:slides.map(s=>({id:1,name:'',hidden:false,text:'',notes:'',parse_error:null,shapes:[],...s}))});
test('literal presentation search keeps page/field identity and bounds work',()=>{
 const model=report([{text:'中文 <x> 中文',notes:'中文备注',hidden:true},{text:'中文',parse_error:'failed'}]);
 expect(searchPresentation(model,'中文')).toEqual({matches:[{slide:0,field:'text',offset:0},{slide:0,field:'text',offset:7},{slide:0,field:'notes',offset:0}],truncated:false});
 expect(searchPresentation(model,'<x>').matches).toHaveLength(1);expect(searchPresentation(model,'')).toEqual({matches:[],truncated:false});
 expect(searchPresentation(report([{text:'a'.repeat(1001)}]),'a')).toMatchObject({truncated:true});
 expect(searchPresentation(report([{text:'a'.repeat(1001)}]),'a').matches).toHaveLength(1000);
 expect(searchPresentation(report([{text:'x'.repeat(4*1024*1024+1)}]),'x')).toEqual({matches:[],truncated:true});
 expect(searchPresentation(report([{text:'Test test'}]),'Test').matches).toHaveLength(1);
});
test('real legacy PPT search jumps between body and notes and clears stale matches',async({page})=>{
 const source=path.resolve('tests/fixtures/apache-poi/basic_test_ppt_file.ppt');
 const presentation=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-ppt',source],{encoding:'utf8',windowsHide:true}));
 await nativeBridge(page,async command=>{if(command==='open_document')return {name:'test.ppt',path:source,inspection:{format:'ppt',text:'',bytes:100,warnings:[]},presentation};throw new Error(command);});
 await page.goto('/');await page.locator('#open').click();
 await page.locator('#slide-search').fill('notes');await page.locator('#slide-search').press('Enter');
 await expect(page.locator('#slide-search-results option')).toHaveCount(2);await expect(page.locator('#slide-search-status')).toContainText('备注');
 await page.locator('#slide-search-results').selectOption('1');await expect(page.locator('#slide-picker')).toHaveValue('1');await expect(page.locator('#slide-notes')).toContainText('notes on page two');
 await page.locator('#slide-search').fill('This is on page 1');await expect(page.locator('#slide-search-results')).toBeDisabled();await page.locator('#slide-search-run').click();
 await expect(page.locator('#slide-picker')).toHaveValue('0');await expect(page.locator('#slide-search-status')).toContainText('正文');
 await page.locator('#slide-search').fill('absent');await page.locator('#slide-search-run').click();await expect(page.locator('#slide-search-status')).toContainText('0 个匹配');await expect(page.locator('#slide-search-results')).toBeDisabled();
});
