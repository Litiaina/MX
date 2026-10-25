import {describe,it,expect} from 'vitest';
import {decimalUnits,formatUnits,journalTotals} from './bookAmounts';
describe('exact book previews',()=>{
  it('keeps values beyond JavaScript safe integers exact',()=>{expect(decimalUnits('90071992547409.93',2)).toBe(9007199254740993n);expect(formatUnits(9007199254740993n,2)).toBe('90071992547409.93');});
  it('does not silently round, normalize a comma or accept float syntax',()=>{for(const value of ['1.001','-1','+1','1e3','1,000','NaN','0',' 1','1.','.1','92233720368547758.08'])expect(decimalUnits(value,2)).toBeNull();});
  it('compares debits and credits without float math',()=>{expect(journalTotals([{debit:'0.1',credit:''},{debit:'0.2',credit:''},{debit:'',credit:'0.30'}],2)).toEqual({debit:'0.30',credit:'0.30',balanced:true});expect(journalTotals([{debit:'1',credit:''},{debit:'',credit:'0.99'}],2)?.balanced).toBe(false);});
  it('rejects ambiguous lines',()=>{expect(journalTotals([{debit:'1',credit:'1'}],2)).toBeNull();expect(journalTotals([{debit:'',credit:''}],2)).toBeNull();});
});
