import { describe, expect, it, vi } from 'vitest';
import { officeResources, officeOperationTimeout, officeTimeoutMessage, checkOfficeInputSize, OFFICE_INPUT_BUDGET } from './resources';
function archive(expanded: number) {
  const bytes=new Uint8Array(68),view=new DataView(bytes.buffer);
  view.setUint32(0,0x02014b50,true);view.setUint32(24,expanded,true);
  view.setUint32(46,0x06054b50,true);view.setUint16(56,1,true);view.setUint32(58,46,true);
  return new Blob([bytes]);
}
describe('bounded Office resource inspection',()=>{
  it('distinguishes compressed file size from expanded workbook complexity',async()=>{
    expect(await officeResources(archive(58*1024**2),'.xlsx')).toEqual({large:true,expandedBytes:58*1024**2});
    expect((await officeResources(archive(1024),'.xlsx')).large).toBe(false);
  });
  it('reads metadata slices, not the complete file or expanded cells',async()=>{
    const blob=archive(100*1024**2),whole=vi.spyOn(blob,'arrayBuffer'),slice=vi.spyOn(blob,'slice');
    await officeResources(blob,'.xlsx');expect(whole).not.toHaveBeenCalled();expect(slice).toHaveBeenCalledTimes(2);
  });
  it('handles truncated/invalid archives without allocating declared directories',async()=>{
    expect(await officeResources(new Blob(['bad']),'.xlsx')).toEqual({large:false,expandedBytes:null});
    const blob=archive(1),bytes=new Uint8Array(await blob.arrayBuffer());new DataView(bytes.buffer).setUint32(58,0xffffffff,true);
    expect((await officeResources(new Blob([bytes]),'.xlsx')).expandedBytes).toBeNull();
  });
  it('gives document parsing/export time, without claiming an unopened file is open',()=>{
    expect(officeOperationTimeout('open')).toBe(600000);expect(officeOperationTimeout('export')).toBe(600000);expect(officeOperationTimeout('focus')).toBe(30000);
    expect(officeTimeoutMessage('open')).toContain('No changes were saved');
    expect(officeTimeoutMessage('open')).not.toContain('still open');expect(officeTimeoutMessage('export')).toContain('not confirmed a save');
  });
  it('rejects impossible editor allocations without restricting Drive uploads',async()=>{
    expect(()=>checkOfficeInputSize(OFFICE_INPUT_BUDGET)).not.toThrow();
    expect(()=>checkOfficeInputSize(OFFICE_INPUT_BUDGET+1)).toThrow('not a Drive upload limit');
    await expect(officeResources(archive(300*1024**2),'.xlsx')).rejects.toThrow('original is unchanged');
  });
});
