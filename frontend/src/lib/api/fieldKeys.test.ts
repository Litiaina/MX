import { describe, expect, it } from 'vitest';
import { ApiError } from './client';
import { fieldKeyFailure, generatedFieldKey } from './fieldKeys';

describe('field key guidance', () => {
  it('generates a valid bounded identifier without changing explicit operator keys', () => {
    expect(generatedFieldKey('Employee classification')).toBe('employee_classification');
    expect(generatedFieldKey('123 Name')).toBe('_name');
    expect(generatedFieldKey('!!!')).toBe('field');
    expect(generatedFieldKey('x'.repeat(100))).toHaveLength(64);
  });
  const conflict = {
    field: { uid:'archived-id', key:'classification', label:'Classification', field_type:'text', active:false, module_uid:'employees', module_name:'Employees' },
    suggested_key:'classification_2'
  };
  it('accepts an actionable archived key conflict', () => {
    expect(fieldKeyFailure(new ApiError('Reserved key',409,{ error:'field_key_conflict', field_key_conflict:conflict }))).toEqual(conflict);
  });
  it('does not invent details for old servers, connectivity errors or malformed payloads', () => {
    for (const reason of [new Error('Disconnected'), new ApiError('Old duplicate error',409,null), new ApiError('Invalid',400,{error:'something_else',field_key_conflict:conflict}),
      new ApiError('Invalid',409,{error:'field_key_conflict',field_key_conflict:{...conflict,field:null}}),
      new ApiError('Invalid',409,{error:'field_key_conflict',field_key_conflict:{...conflict,suggested_key:'bad key'}}),
      new ApiError('Invalid',409,{error:'field_key_conflict',field_key_conflict:{...conflict,field:{...conflict.field,active:'false'}}})]) {
      expect(fieldKeyFailure(reason)).toBeNull();
    }
  });
});
