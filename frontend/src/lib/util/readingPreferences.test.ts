import { describe, expect, it } from 'vitest';
import { columnPresentation, columnTone, fieldIsBold, withBoldFields, withColumnPresentation } from './readingPreferences';

describe('personal module field emphasis', () => {
  it('scopes stable field IDs to their module, not their labels', () => {
    const preferences = withBoldFields({}, 'employees', ['name-uid']);
    expect(fieldIsBold(preferences, 'employees', 'name-uid')).toBe(true);
    expect(fieldIsBold(preferences, 'requests', 'name-uid')).toBe(false);
    expect(fieldIsBold(preferences, 'employees', 'other-uid')).toBe(false);
  });
  it('deduplicates fields and retains unrelated module preferences without mutation', () => {
    const original = { employees: { bold_fields: ['a'] } };
    const next = withBoldFields(original, 'requests', ['c', 'b', 'c']);
    expect(next).toEqual({ employees: { bold_fields: ['a'] }, requests: { bold_fields: ['b', 'c'] } });
    expect(original).toEqual({ employees: { bold_fields: ['a'] } });
    expect(withBoldFields(next, 'requests', [])).toEqual(original);
  });
  it('preserves column appearance when bold fields change and scopes it per module', () => {
    const original = withColumnPresentation({}, 'employees', { name: {style:'badge',tone:'blue'} });
    const bold = withBoldFields(original, 'employees', ['name']);
    expect(bold.employees.columns).toEqual(original.employees.columns);
    expect(withBoldFields(bold, 'employees', []).employees.columns).toEqual(original.employees.columns);
    expect(withColumnPresentation(bold, 'employees', {}).employees).toEqual({bold_fields:['name']});
    expect(columnPresentation(original,'employees',{uid:'name',field_type:'text'})).toEqual({style:'badge',tone:'blue'});
    expect(columnPresentation(original,'other',{uid:'name',field_type:'text'})).toEqual({style:'text',tone:'auto'});
    expect(columnPresentation({},'employees',{uid:'status',field_type:'select'})).toEqual({style:'badge',tone:'auto'});
    expect(original.employees.bold_fields).toEqual([]);
  });
  it('uses stable category colors and no color for ordinary unconfigured text', () => {
    const automatic = {style:'badge' as const,tone:'auto' as const};
    expect(columnTone(automatic,'Approved','select')).toBe(columnTone(automatic,'Approved','select'));
    expect(columnTone({...automatic,style:'text'},'private value','text')).toBe('none');
    expect(columnTone(automatic,true,'boolean')).toBe('green');
    expect(columnTone({...automatic,tone:'rose'},'anything','select')).toBe('rose');
  });
});
