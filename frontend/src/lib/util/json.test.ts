import { describe, expect, it } from 'vitest';
import { cloneJson } from './json';

describe('cloneJson', () => {
  it('clones proxied API data without retaining the proxy', () => {
    const source = new Proxy({ control_no: 4, subject: 'Existing value' }, {});
    const result = cloneJson(source);

    expect(result).toEqual({ control_no: 4, subject: 'Existing value' });
    expect(result).not.toBe(source);
  });
});
