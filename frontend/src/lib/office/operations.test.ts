import { describe, expect, it } from 'vitest';
import { OfficeOperations } from './operations';

describe('Office local work ordering', () => {
  it('queues Save behind a checkpoint, without losing the requested save', async () => {
    const operations = new OfficeOperations();
    const calls: string[] = [];
    let release!: () => void;
    const blocked = new Promise<void>(resolve => { release = resolve; });
    const checkpoint = operations.run(async () => { calls.push('checkpoint'); await blocked; calls.push('stored'); });
    const save = operations.run(async () => { calls.push('save'); });
    await Promise.resolve();
    expect(calls).toEqual(['checkpoint']);
    release(); await Promise.all([checkpoint, save]);
    expect(calls).toEqual(['checkpoint', 'stored', 'save']);
  });
  it('keeps recovery possible after a failed export', async () => {
    const operations = new OfficeOperations();
    await expect(operations.run(async () => { throw new Error('export failed'); })).rejects.toThrow('export failed');
    await expect(operations.run(async () => 'download')).resolves.toBe('download');
  });
  it('waits for queued writes before removing a retained local copy', async () => {
    const operations = new OfficeOperations();
    const calls: string[] = [];
    void operations.run(async () => { calls.push('store'); });
    await operations.idle(); calls.push('remove');
    expect(calls).toEqual(['store', 'remove']);
  });
});
