import {describe,it,expect} from 'vitest';
import {ApiError} from './client';
import {archiveFailure} from './fieldArchive';
describe('field archive blockers',()=>{
  it('accepts structured archive blockers for the requested field only',()=>{
    const archive_check={field:{uid:'source',label:'Name'},blockers:[{module_name:'Inventory',field_label:'Issuer'}]};
    const error=new ApiError('Cannot archive',400,{error:'field_archive_blocked',archive_check});
    expect(archiveFailure(error,'source')).toEqual(archive_check);
    expect(archiveFailure(error,'other')).toBeNull();
  });
  it('keeps network and older unstructured failures distinct',()=>{
    expect(archiveFailure(new Error('Network'),'source')).toBeNull();
    expect(archiveFailure(new ApiError('Configure dependencies',400,{response:'Configure dependencies'}),'source')).toBeNull();
    expect(archiveFailure(new ApiError('Bad payload',400,{error:'field_archive_blocked',archive_check:{field:{uid:'source'},blockers:null}}),'source')).toBeNull();
  });
});
