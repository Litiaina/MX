import {describe,it,expect} from 'vitest';
import {applyRelationshipPreview} from './relationshipAutofill';
describe('relationship auto-fill',()=>{
  it('fills configured fields and calculated lookups without touching unrelated input',()=>{
    expect(applyRelationshipPreview({classification:'Old',stock:'Medicine'},{classification:'Old'}, {values:{classification:'JO'},computed:{position:['Nurse']}})).toEqual({classification:'JO',stock:'Medicine',position:['Nurse']});
  });
  it('preserves typing while the linked values were loading',()=>{
    expect(applyRelationshipPreview({classification:'User override'},{classification:'Old'}, {values:{classification:'JO'},computed:{}}).classification).toBe('User override');
  });
  it('clears copied values on deselection, including zeros and booleans',()=>{
    expect(applyRelationshipPreview({count:0,enabled:false},{count:0,enabled:false}, {values:{count:null,enabled:null},computed:{lookup:[]}})).toEqual({count:null,enabled:null,lookup:[]});
  });
});
