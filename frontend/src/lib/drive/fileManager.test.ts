import { expect, it, vi } from 'vitest';
import { selection, intersects, droppedFiles, activityLabel } from './fileManager';
it('selects, toggles and shift-ranges without losing additive selection',()=>{
  const order=['a','b','c','d'];
  expect([...selection(new Set(['d']),order,'a',null,false,false)]).toEqual(['a']);
  expect([...selection(new Set(['a']),order,'c','a',true,false)]).toEqual(['a','c']);
  expect([...selection(new Set(['a','c']),order,'a','c',true,false)]).toEqual(['c']);
  expect([...selection(new Set(['d']),order,'c','a',false,true)]).toEqual(['a','b','c']);
  expect([...selection(new Set(['d']),order,'a','c',true,true)]).toEqual(['d','a','b','c']);
});
it('marquee includes intersecting items, not touching/outside items',()=>{
  const box={left:10,top:10,right:20,bottom:20};
  expect(intersects(box,{left:15,top:15,right:25,bottom:25})).toBe(true);
  expect(intersects(box,{left:20,top:10,right:30,bottom:20})).toBe(false);
});
it('drains every folder reader page and retains empty directories without reading file bytes',async()=>{
  const file=new File(['hello'],'one.txt'); const read=vi.spyOn(file,'arrayBuffer');
  const leaf={isFile:true,isDirectory:false,name:'one.txt',file:(ok:(f:File)=>void)=>ok(file)};
  const empty={isFile:false,isDirectory:true,name:'empty',createReader:()=>({readEntries:(ok:(v:unknown[])=>void)=>ok([])})};
  const pages=[[leaf],[empty],[]]; const folder={isFile:false,isDirectory:true,name:'References',createReader:()=>({readEntries:(ok:(v:unknown[])=>void)=>ok(pages.shift()!)})};
  const data={items:[{kind:'file',webkitGetAsEntry:()=>folder,getAsFile:()=>null}],files:[]} as unknown as DataTransfer;
  const result=await droppedFiles(data);
  expect(result.files).toEqual([{file,relativePath:'References/one.txt'}]); expect(result.folders).toEqual(['References','References/empty']); expect(read).not.toHaveBeenCalled();
});
it('supports ordinary file drops without webkit directory APIs',async()=>{
  const file=new File(['image'],'paste.png');
  expect((await droppedFiles({items:[{kind:'file',getAsFile:()=>file}],files:[file]} as unknown as DataTransfer)).files[0].file).toBe(file);
});
it('prefers an available File over a WebKit filesystem entry that cannot be read',async()=>{
  const file=new File(['bytes'],'drop.txt');const read=vi.fn();
  const data={items:[{kind:'file',getAsFile:()=>file,webkitGetAsEntry:()=>({isFile:true,isDirectory:false,name:file.name,file:read})}],files:[file]} as unknown as DataTransfer;
  expect((await droppedFiles(data)).files[0].file).toBe(file);expect(read).not.toHaveBeenCalled();
});
it('does not silently lose an unsupported folder',async()=>{
  await expect(droppedFiles({items:[{kind:'file',getAsFile:()=>null}],files:[]} as unknown as DataTransfer)).rejects.toThrow('Use Upload folder');
});
it('gives common activity events plain-language descriptions',()=>{
  expect(activityLabel('item.copied')).toBe('Created a copy'); expect(activityLabel('item.trash')).toBe('Moved this item to trash');
});
