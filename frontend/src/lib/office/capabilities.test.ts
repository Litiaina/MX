import { afterEach, describe, expect, it, vi } from 'vitest';

afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); vi.resetModules(); });
function graphicsWorker(supported: boolean) {
  const terminate = vi.fn();
  let workers = 0;
  class TestWorker {
    onmessage?: (event: {data: boolean}) => void;
    onerror?: (event: Event) => void;
    constructor() { workers++; }
    postMessage() { queueMicrotask(() => this.onmessage?.({data:supported})); }
    terminate = terminate;
  }
  const canvas = {};
  vi.stubGlobal('Worker', TestWorker);
  vi.stubGlobal('OffscreenCanvas', class {});
  vi.stubGlobal('HTMLCanvasElement', class { transferControlToOffscreen() { return canvas; } });
  vi.stubGlobal('document', {createElement:() => ({transferControlToOffscreen:() => canvas})});
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:test');
  const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  return {terminate,revoke,workers:() => workers};
}
describe('Office graphics preflight', () => {
  it('does not confuse OffscreenCanvas existence with worker WebGL support', async () => {
    const native = graphicsWorker(false);
    const {checkOfficeGraphics} = await import('./capabilities');
    await expect(checkOfficeGraphics()).rejects.toThrow('WebGL in an Office worker');
    expect(native.terminate).toHaveBeenCalledOnce(); expect(native.revoke).toHaveBeenCalledOnce();
  });
  it('runs one real-capability probe per tab and releases its worker', async () => {
    const native = graphicsWorker(true);
    const {checkOfficeGraphics} = await import('./capabilities');
    await Promise.all([checkOfficeGraphics(),checkOfficeGraphics()]);
    expect(native.workers()).toBe(1); expect(native.terminate).toHaveBeenCalledOnce();
  });
  it('gives a useful fallback on browsers without graphics workers', async () => {
    vi.stubGlobal('Worker', undefined);
    const {checkOfficeGraphics} = await import('./capabilities');
    await expect(checkOfficeGraphics()).rejects.toThrow('Download a copy');
  });
});
