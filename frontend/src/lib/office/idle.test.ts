import {describe, expect, it} from 'vitest';
import {officeIdleDelay} from './idle';
describe('Office non-interrupting background preparation', () => {
  it('waits for a real 15-second interaction pause, not just a document modification', () => {
    expect(officeIdleDelay(20_000, 18_000, -Infinity, 0)).toBe(13_000);
    expect(officeIdleDelay(33_000, 18_000, -Infinity, 0)).toBe(0);
  });
  it('never starts during a held drag, key or text composition', () => {
    expect(officeIdleDelay(60_000, 0, -Infinity, 0, true)).toBe(1000);
  });
  it('spaces exports even for cheap documents and backs off after expensive work', () => {
    expect(officeIdleDelay(25_000, 0, 0, 50)).toBe(5000);
    expect(officeIdleDelay(45_000, 0, 0, 10_000)).toBe(15_000);
    expect(officeIdleDelay(60_000, 0, 0, 100_000)).toBe(0);
  });
  it('cancels permission to prepare if interaction resumes while waiting in a work queue', () => {
    expect(officeIdleDelay(30_000, 0, -Infinity, 0)).toBe(0);
    expect(officeIdleDelay(30_001, 30_000, -Infinity, 0)).toBe(14_999);
  });
});
