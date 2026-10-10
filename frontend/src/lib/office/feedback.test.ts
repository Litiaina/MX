import { describe, expect, it } from 'vitest';
import { offlinePreparationSummary, savedToMx } from './feedback';

describe('Office save and offline feedback', () => {
  it('identifies the acknowledged server revision and time', () => {
    const time = new Date(2026, 9, 10, 16, 12, 30).getTime();
    expect(savedToMx(7, time, false)).toBe(`Saved to MX · revision 7 · ${new Date(time).toLocaleTimeString()}`);
    expect(savedToMx(7, time, true)).toContain('newer edits still need saving');
  });
  it('separates a rejected certificate from a server-save failure', () => {
    const summary = offlinePreparationSummary('Failed to register a ServiceWorker: An SSL certificate error occurred.');
    expect(summary).toContain('HTTPS certificate');
    expect(summary).not.toContain('register a ServiceWorker');
    expect(summary).toContain('Editing and saving can still work');
  });
  it('does not claim offline support when preparation fails for another reason', () => {
    expect(offlinePreparationSummary('Cache storage quota exceeded')).toContain('Offline reopening is not ready');
  });
});
