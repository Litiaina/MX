import { describe, it, expect } from 'vitest';
import { summaryValue } from './columnSummaries';
describe('record summary display',()=>{
  it('distinguishes unavailable or empty results from an actual zero',()=>{
    expect(summaryValue(undefined)).toBe('Unavailable');
    expect(summaryValue({function:'average',value:null,count:0,decimals:2})).toBe('—');
    expect(summaryValue({function:'sum',value:0,count:0,decimals:2})).toBe((0).toLocaleString(undefined,{minimumFractionDigits:2,maximumFractionDigits:2}));
  });
  it('uses configured precision and whole-number counts',()=>{
    expect(summaryValue({function:'sum',value:0.1+0.2,count:2,decimals:2})).toBe((0.3).toLocaleString(undefined,{minimumFractionDigits:2,maximumFractionDigits:2}));
    expect(summaryValue({function:'count',value:1234,count:1234,decimals:8})).toBe((1234).toLocaleString());
  });
});
