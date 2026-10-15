import {describe,expect,it} from 'vitest';
import {recoveredUpload} from './recovery';
describe('recovered Office payloads',()=>{
  it('materializes bytes without changing upload identity',async()=>{
    const original=new File([new Uint8Array([1,2,3,4])],'Report.xlsx',{type:'application/octet-stream',lastModified:12345});
    const copy=(await recoveredUpload(original))!;
    expect(copy).not.toBe(original);expect(copy.name).toBe(original.name);expect(copy.type).toBe(original.type);expect(copy.lastModified).toBe(original.lastModified);expect(copy.size).toBe(original.size);
    expect([...new Uint8Array(await copy.arrayBuffer())]).toEqual([1,2,3,4]);
  });
  it('does not accept truncated bytes',async()=>{
    await expect(recoveredUpload({size:5,arrayBuffer:async()=>new ArrayBuffer(2)} as File)).rejects.toThrow('incomplete');
    expect(await recoveredUpload()).toBeUndefined();
  });
  it('reuses a materialized matching snapshot without rereading an IDB File',async()=>{
    const original=new File([new Uint8Array([1,2,3])],'Report.xlsx',{type:'application/octet-stream',lastModified:12345});
    Object.defineProperty(original,'arrayBuffer',{value:async()=>{throw new Error('must not reread IDB');}});
    const copy=(await recoveredUpload(original,new Blob([new Uint8Array([1,2,3])])))!;
    expect(copy.lastModified).toBe(12345);expect([...new Uint8Array(await copy.arrayBuffer())]).toEqual([1,2,3]);
  });
});
