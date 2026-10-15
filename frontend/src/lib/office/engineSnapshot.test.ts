import { afterEach, describe, expect, it, vi } from 'vitest';
import { OfficeEngine } from './engine';
afterEach(()=>vi.unstubAllGlobals());
describe('Office snapshot preparation',()=>{
  it('reuses an unchanged prepared snapshot and releases temporary export bytes',async()=>{
    let bytes=new Uint8Array([1,2,3]);const readFile=vi.fn(()=>bytes),unlink=vi.fn();
    vi.stubGlobal('window',{FS:{readFile,unlink}});
    const engine=new OfficeEngine(()=>{},()=>{},()=>{},()=>{});
    const request=vi.spyOn(engine as unknown as {request:(cmd:string,extra:Record<string,unknown>)=>Promise<Record<string,unknown>>},'request');
    request.mockResolvedValueOnce({generation:1}).mockResolvedValueOnce({generation:1,reused:true}).mockResolvedValueOnce({generation:2});
    const first=await engine.snapshot('.xlsx');bytes[0]=9;
    expect([...new Uint8Array(await first.blob.arrayBuffer())]).toEqual([1,2,3]);
    expect(await engine.snapshot('.xlsx')).toBe(first);
    expect(request.mock.calls[1][1].reuseGeneration).toBe(1);
    bytes=new Uint8Array([4,5,6]);const next=await engine.snapshot('.xlsx');
    expect([...new Uint8Array(await next.blob.arrayBuffer())]).toEqual([4,5,6]);
    expect(readFile).toHaveBeenCalledTimes(2);expect(unlink).toHaveBeenCalledTimes(2);
  });
  it('does not claim a prepared copy when exporting fails',async()=>{
    const readFile=vi.fn();vi.stubGlobal('window',{FS:{readFile}});
    const engine=new OfficeEngine(()=>{},()=>{},()=>{},()=>{});
    vi.spyOn(engine as unknown as {request:(cmd:string,extra:Record<string,unknown>)=>Promise<Record<string,unknown>>},'request').mockRejectedValueOnce(new Error('export failed'));
    await expect(engine.snapshot('.docx')).rejects.toThrow('export failed');expect(readFile).not.toHaveBeenCalled();
  });
});
