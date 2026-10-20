import { describe, expect, it } from 'vitest';
import { fieldIsBold, withBoldFields } from './readingPreferences';

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
});
