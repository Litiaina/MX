import { describe, it, expect } from 'vitest';
import { officeExtension, officeFormats } from './formats';
import { canOpenOffice, canEditOffice } from './launch';
import type { DriveItem } from '../api/drive';
describe('Office formats and stable source names',()=>{
  it('uses explicit round-trip filters',()=>{expect(officeExtension('Budget.XLSX')).toBe('.xlsx');expect(officeFormats['.xlsx'].filter).toBe('Calc MS Excel 2007 XML');expect(officeFormats['.docx'].filter).toBe('Office Open XML Text');});
  it('does not silently edit macro-enabled or disguised executables',()=>{expect(officeExtension('macros.xlsm')).toBe('');expect(officeExtension('letter.docx.exe')).toBe('');expect(officeExtension('plain.pdf')).toBe('');});
  it('uses the original format after metadata-only rename',()=>{const item={kind:'file',name:'Quarterly budget',original_file_name:'original.xlsx',trashed_at:null} as DriveItem;expect(canOpenOffice(item)).toBe(true);expect(canOpenOffice({...item,trashed_at:1})).toBe(false);expect(canOpenOffice({...item,kind:'folder'})).toBe(false);});
  it('keeps presentations and viewer grants read-only until editing is verified',()=>{
    const item={kind:'file',name:'Slides.pptx',permission:'owner',trashed_at:null} as DriveItem;
    expect(canOpenOffice(item)).toBe(true);expect(canEditOffice(item)).toBe(false);
    expect(canEditOffice({...item,name:'Budget.xlsx'})).toBe(true);
    expect(canEditOffice({...item,name:'Budget.xlsx',permission:'viewer'})).toBe(false);
  });
});
