import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { apiFile, apiJson, apiUpload, apiUploadPart, ApiError, ConnectionError, hasAuthTokens, storeAuthTokens, clearAuthTokens } from './client';
import { currentSession, restoreSession } from '../auth/session';

class MemoryStorage {
  private data = new Map<string, string>();
  getItem(key: string) { return this.data.get(key) ?? null; }
  setItem(key: string, value: string) { this.data.set(key, value); }
  removeItem(key: string) { this.data.delete(key); }
}
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal('sessionStorage', new MemoryStorage());
  vi.stubGlobal('localStorage', new MemoryStorage());
  vi.stubGlobal('window', new EventTarget());
  currentSession.set(null);
  storeAuthTokens({ access_token: 'access', refresh_token: 'refresh' } as never);
});
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

describe('network-safe API requests', () => {
  it('retries transient reads and consumes the successful JSON body', async () => {
    const fetcher = vi.fn().mockResolvedValueOnce(json({}, 503)).mockRejectedValueOnce(new TypeError('reset')).mockResolvedValueOnce(json({ ok: true }));
    vi.stubGlobal('fetch', fetcher);
    const promise = apiJson('/read');
    await vi.runAllTimersAsync();
    expect(await promise).toEqual({ ok: true });
    expect(fetcher).toHaveBeenCalledTimes(3);
  });

  it('never automatically replays a write without an explicit replay contract', async () => {
    const fetcher = vi.fn().mockRejectedValue(new TypeError('response lost after commit'));
    vi.stubGlobal('fetch', fetcher);
    await expect(apiJson('/write', { method: 'POST' })).rejects.toMatchObject({ name: 'ConnectionError', uncertainWrite: true });
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it('replays deduplicated writes with exactly the same body', async () => {
    const fetcher = vi.fn().mockRejectedValueOnce(new TypeError('reset')).mockResolvedValueOnce(json({ uid: 'one' }));
    vi.stubGlobal('fetch', fetcher);
    const body = JSON.stringify({ operation_uid: 'stable', values: { name: 'one' } });
    const promise = apiJson('/write', { method: 'POST', body, retrySafe: true, maxAttempts: 2 });
    await vi.runAllTimersAsync();
    expect(await promise).toEqual({ uid: 'one' });
    expect(fetcher.mock.calls.map((call) => call[1].body)).toEqual([body, body]);
  });

  it('bounds a silent request with cancellation and a finite attempt budget', async () => {
    const fetcher = vi.fn((_path, options) => new Promise((_resolve, reject) => {
      options.signal.addEventListener('abort', () => reject(options.signal.reason));
    }));
    vi.stubGlobal('fetch', fetcher);
    const assertion = expect(apiJson('/hung', { timeoutMs: 50, maxAttempts: 2 })).rejects.toBeInstanceOf(ConnectionError);
    await vi.runAllTimersAsync();
    await assertion;
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(fetcher.mock.calls.every((call) => call[1].signal.aborted)).toBe(true);
  });

  it('does not retry validation or authorization failures', async () => {
    const fetcher = vi.fn(async () => json({ response: 'invalid' }, 400));
    vi.stubGlobal('fetch', fetcher);
    await expect(apiJson('/invalid')).rejects.toMatchObject({ status: 400 });
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it('rejects a captive portal HTML response instead of treating it as API data', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('<html>Gateway login</html>', { headers: { 'Content-Type': 'text/html' } })));
    await expect(apiJson('/read')).rejects.toMatchObject({ status: 502 });
    expect(hasAuthTokens()).toBe(true);
  });

  it('keeps the deadline active while the JSON body stalls after response headers', async () => {
    vi.stubGlobal('fetch', vi.fn(async (_path, options) => new Response(new ReadableStream({ start(controller) {
      options.signal.addEventListener('abort', () => controller.error(options.signal.reason));
    } }))));
    const assertion = expect(apiJson('/body-hung', { timeoutMs: 50, maxAttempts: 1 })).rejects.toBeInstanceOf(ConnectionError);
    await vi.advanceTimersByTimeAsync(100); await assertion;
  });

  it('respects caller cancellation without retrying', async () => {
    const controller = new AbortController();
    const fetcher = vi.fn((_path, options) => new Promise((_resolve, reject) => options.signal.addEventListener('abort', () => reject(options.signal.reason))));
    vi.stubGlobal('fetch', fetcher);
    const assertion = expect(apiJson('/cancel', { signal: controller.signal })).rejects.toMatchObject({ name: 'AbortError' });
    controller.abort();
    await assertion;
    await vi.runAllTimersAsync();
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
});

describe('buffered file recovery', () => {
  it('cancels a stalled body instead of reporting a truncated file as successful', async () => {
    const cancel = vi.fn();
    vi.stubGlobal('fetch', vi.fn(async () => new Response(new ReadableStream({ cancel }))));
    const assertion = expect(apiFile('/file', { maxAttempts: 1 })).rejects.toBeInstanceOf(ConnectionError);
    await vi.advanceTimersByTimeAsync(60001); await assertion;
    expect(cancel).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
  });
  it('keeps slow downloads alive while chunks arrive and preserves headers and every byte', async () => {
    let body!: ReadableStreamDefaultController<Uint8Array>;
    vi.stubGlobal('fetch', vi.fn(async () => new Response(new ReadableStream({ start(controller) { body = controller; } }), { headers: { 'Content-Type': 'text/plain', 'Content-Disposition': 'attachment; filename="test.txt"' } })));
    const result = apiFile('/slow');
    await vi.advanceTimersByTimeAsync(50000); body.enqueue(new TextEncoder().encode('first'));
    await vi.advanceTimersByTimeAsync(50000); body.enqueue(new TextEncoder().encode('second'));
    body.close();
    const file = await result;
    expect(await file.blob.text()).toBe('firstsecond');
    expect(file.headers.get('content-disposition')).toContain('test.txt');
    expect(vi.getTimerCount()).toBe(0);
  });
  it('retries a broken read from the beginning without retaining partial bytes', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(new Response(new ReadableStream({ start(controller) { controller.error(new TypeError('reset')); } }))).mockResolvedValueOnce(new Response('complete')));
    const result = apiFile('/retry');
    await vi.runAllTimersAsync();
    expect(await (await result).blob.text()).toBe('complete');
  });
});

class FakeUpload {
  static instances: FakeUpload[] = [];
  upload: { onprogress?: (event: unknown) => void; onload?: () => void } = {};
  onabort?: () => void; onerror?: () => void; ontimeout?: () => void;
  onload?: () => Promise<void>;
  status = 200; responseText = '';
  timeout = 0; aborted = false;
  constructor() { FakeUpload.instances.push(this); }
  open() {} setRequestHeader(_key: string, _value: string) {} send() {}
  getResponseHeader() { return 'text/html'; }
  abort() { this.aborted = true; this.onabort?.(); }
}
describe('upload progress deadlines', () => {
  beforeEach(() => { FakeUpload.instances = []; vi.stubGlobal('XMLHttpRequest', FakeUpload); });
  it('sends native Blob PUT parts with in-flight progress and identical-body retries',async()=>{
    const opening=vi.spyOn(FakeUpload.prototype,'open'); const sending=vi.spyOn(FakeUpload.prototype,'send');
    const body=new Blob(['immutable part']); const progress=vi.fn();
    const result=apiUploadPart('/parts/0',body,progress);
    FakeUpload.instances[0].upload.onprogress?.({loaded:4,total:14,lengthComputable:true});
    expect(progress).toHaveBeenCalledWith(4,14);
    FakeUpload.instances[0].status=503; FakeUpload.instances[0].responseText='{}'; await FakeUpload.instances[0].onload?.();
    await vi.advanceTimersByTimeAsync(1300);
    expect(FakeUpload.instances).toHaveLength(2); expect(progress).toHaveBeenCalledWith(0,body.size);
    FakeUpload.instances[1].responseText='{"stored":true}'; await FakeUpload.instances[1].onload?.();
    expect(await result).toEqual({stored:true}); expect(opening.mock.calls).toEqual([['PUT','/parts/0'],['PUT','/parts/0']]);
    expect(sending.mock.calls).toEqual([[body],[body]]); expect(vi.getTimerCount()).toBe(0);
    opening.mockRestore(); sending.mockRestore();
  });
  it('does not retry an immutable part on conflict or after explicit cancellation',async()=>{
    const conflict=expect(apiUploadPart('/parts/0',new Blob(['x']))).rejects.toMatchObject({status:409});
    FakeUpload.instances[0].status=409; FakeUpload.instances[0].responseText='{}'; await FakeUpload.instances[0].onload?.(); await conflict;
    expect(FakeUpload.instances).toHaveLength(1);
    const controller=new AbortController(); const cancelled=expect(apiUploadPart('/parts/1',new Blob(['x']),undefined,controller.signal)).rejects.toMatchObject({name:'AbortError'});
    controller.abort(); await cancelled; await vi.advanceTimersByTimeAsync(15000); expect(FakeUpload.instances).toHaveLength(2);
    expect(vi.getTimerCount()).toBe(0);
  });
  it('refreshes expired authentication without changing the part method, slot or bytes',async()=>{
    vi.stubGlobal('fetch',vi.fn(async()=>json({access_token:'renewed',refresh_token:'next'})));
    const opening=vi.spyOn(FakeUpload.prototype,'open'); const sending=vi.spyOn(FakeUpload.prototype,'send');
    const body=new Blob(['original bytes']); const result=apiUploadPart('/parts/7',body);
    FakeUpload.instances[0].status=401; FakeUpload.instances[0].responseText='{}'; const refreshing=FakeUpload.instances[0].onload?.();
    await vi.advanceTimersByTimeAsync(0);
    expect(FakeUpload.instances).toHaveLength(2);
    FakeUpload.instances[1].responseText='{"stored":true}'; await FakeUpload.instances[1].onload?.(); await refreshing;
    expect(await result).toEqual({stored:true}); expect(opening.mock.calls).toEqual([['PUT','/parts/7'],['PUT','/parts/7']]);
    expect(sending.mock.calls).toEqual([[body],[body]]); expect(vi.getTimerCount()).toBe(0);
    opening.mockRestore(); sending.mockRestore();
  });
  it('bounds multipart connection retries and cleans every deadline after exhaustion',async()=>{
    const assertion=expect(apiUploadPart('/parts/0',new Blob(['same']))).rejects.toBeInstanceOf(ConnectionError);
    for(let attempt=0;attempt<5;attempt++){expect(FakeUpload.instances).toHaveLength(attempt+1);FakeUpload.instances[attempt].onerror?.();await vi.advanceTimersByTimeAsync(13000);}
    await assertion; expect(FakeUpload.instances).toHaveLength(5); expect(vi.getTimerCount()).toBe(0);
  });
  it('aborts a silent upload and reports an uncertain outcome without replaying', async () => {
    const assertion = expect(apiUpload('/upload', new FormData())).rejects.toMatchObject({ name: 'ConnectionError', uncertainWrite: true });
    await vi.advanceTimersByTimeAsync(60001); await assertion;
    expect(FakeUpload.instances).toHaveLength(1);
    expect(FakeUpload.instances[0].aborted).toBe(true);
  });
  it('gives storage processing a separate bounded deadline after browser upload completes', async () => {
    const assertion = expect(apiUpload('/upload', new FormData())).rejects.toBeInstanceOf(ConnectionError);
    FakeUpload.instances[0].upload.onload?.();
    await vi.advanceTimersByTimeAsync(60001);
    expect(FakeUpload.instances[0].aborted).toBe(false);
    await vi.advanceTimersByTimeAsync(240000); await assertion;
    expect(FakeUpload.instances[0].aborted).toBe(true);
  });
  it('honors cancellation and removes its deadline', async () => {
    const controller = new AbortController();
    const assertion = expect(apiUpload('/upload', new FormData(), undefined, true, controller.signal)).rejects.toMatchObject({ name: 'AbortError' });
    controller.abort(); await assertion;
    expect(vi.getTimerCount()).toBe(0);
  });
  it('does not acknowledge an upload when a gateway returns HTML with HTTP 200', async () => {
    const assertion = expect(apiUpload('/upload', new FormData())).rejects.toMatchObject({ name: 'ConnectionError', uncertainWrite: true });
    FakeUpload.instances[0].responseText = '<html>Gateway login</html>';
    await FakeUpload.instances[0].onload?.();
    await assertion;
    expect(FakeUpload.instances).toHaveLength(1);
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe('session survival during an outage', () => {
  it('retains live-document authorization when browser session storage disappears', async () => {
    sessionStorage.removeItem('mx_access_token');sessionStorage.removeItem('mx_refresh_token');
    const fetcher=vi.fn(async(_path: string,_options: {headers: Headers})=>json({ok:true}));vi.stubGlobal('fetch',fetcher);
    expect(await apiJson('/after-browser-recovery')).toEqual({ok:true});
    expect(fetcher.mock.calls[0][1].headers.get('Authorization')).toBe('Bearer access');
    expect(hasAuthTokens()).toBe(true);
  });
  it('keeps native part retries authenticated after browser storage recovery', async () => {
    FakeUpload.instances=[];vi.stubGlobal('XMLHttpRequest',FakeUpload);
    const header=vi.spyOn(FakeUpload.prototype,'setRequestHeader');
    const result=apiUploadPart('/parts/recovery',new Blob(['same bytes']));
    FakeUpload.instances[0].onerror?.();
    sessionStorage.removeItem('mx_access_token');sessionStorage.removeItem('mx_refresh_token');
    await vi.advanceTimersByTimeAsync(1300);
    FakeUpload.instances[1].responseText='{}';await FakeUpload.instances[1].onload?.();await result;
    expect(header.mock.calls.filter(([key])=>key==='Authorization')).toEqual([['Authorization','Bearer access'],['Authorization','Bearer access']]);
    header.mockRestore();
  });
  it('signs out even when unavailable storage cannot remove old persisted credentials', () => {
    vi.spyOn(sessionStorage,'removeItem').mockImplementation(()=>{throw new Error('Storage unavailable');});
    vi.spyOn(localStorage,'removeItem').mockImplementation(()=>{throw new Error('Storage unavailable');});
    clearAuthTokens();expect(hasAuthTokens()).toBe(false);
    expect(sessionStorage.getItem('mx_access_token')).toBe('access');
    storeAuthTokens({access_token:'replacement',refresh_token:'next'} as never);expect(hasAuthTokens()).toBe(true);
  });
  it('can replace the live session while browser persistence is unavailable', async () => {
    vi.spyOn(sessionStorage,'setItem').mockImplementation(()=>{throw new Error('Storage unavailable');});
    storeAuthTokens({access_token:'replacement',refresh_token:'next'} as never);
    const fetcher=vi.fn(async(_path: string,_options: {headers: Headers})=>json({ok:true}));vi.stubGlobal('fetch',fetcher);
    await apiJson('/replacement');expect(fetcher.mock.calls[0][1].headers.get('Authorization')).toBe('Bearer replacement');
    clearAuthTokens();expect(hasAuthTokens()).toBe(false);
  });
  it('retains credentials when restoring the session receives 503', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => json({ response: 'maintenance' }, 503)));
    const assertion = expect(restoreSession()).rejects.toBeInstanceOf(ApiError);
    await vi.runAllTimersAsync(); await assertion;
    expect(hasAuthTokens()).toBe(true);
    expect(get(currentSession)).toBeNull();
  });

  it('preserves credentials if token refresh is temporarily unavailable', async () => {
    const fetcher = vi.fn(async (path) => json({}, path.includes('/refresh') ? 503 : 401));
    vi.stubGlobal('fetch', fetcher);
    const assertion = expect(restoreSession()).rejects.toMatchObject({ status: 503 });
    await vi.runAllTimersAsync(); await assertion;
    expect(hasAuthTokens()).toBe(true);
  });

  it('signs out only when refresh is definitively rejected', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => json({}, 401)));
    expect(await restoreSession()).toBeNull();
    expect(hasAuthTokens()).toBe(false);
  });

  it('shares a single refresh between concurrent expired requests', async () => {
    const fetcher = vi.fn(async (path, options) => path.includes('/refresh')
      ? json({ access_token: 'new', refresh_token: 'next' })
      : json({ ok: true }, options.headers.get('Authorization') === 'Bearer new' ? 200 : 401));
    vi.stubGlobal('fetch', fetcher);
    expect(await Promise.all([apiJson('/a'), apiJson('/b'), apiJson('/c')])).toEqual([{ ok: true }, { ok: true }, { ok: true }]);
    expect(fetcher.mock.calls.filter((call) => call[0].includes('/refresh'))).toHaveLength(1);
  });

  it('does not resurrect credentials after sign-out during a delayed refresh', async () => {
    let finish!: (value: Response) => void;
    const fetcher = vi.fn(async (path) => path.includes('/refresh') ? new Promise<Response>((resolve) => finish = resolve) : json({}, 401));
    vi.stubGlobal('fetch', fetcher);
    const assertion = expect(apiJson('/a')).rejects.toThrow('active session changed');
    await vi.waitFor(() => expect(finish).toBeDefined());
    clearAuthTokens();
    finish(json({ access_token: 'new', refresh_token: 'next' }));
    await assertion;
    expect(hasAuthTokens()).toBe(false);
  });
  it('does not clear a replacement login when a delayed old refresh is rejected', async () => {
    let finish!: (value: Response) => void;
    vi.stubGlobal('fetch', vi.fn(async (path) => path.includes('/refresh') ? new Promise<Response>((resolve) => finish = resolve) : json({}, 401)));
    const assertion = expect(restoreSession()).rejects.toThrow('active session changed');
    await vi.waitFor(() => expect(finish).toBeDefined());
    storeAuthTokens({ access_token: 'replacement', refresh_token: 'replacement-refresh' } as never);
    finish(json({}, 401));
    await assertion;
    expect(sessionStorage.getItem('mx_access_token')).toBe('replacement');
    expect(sessionStorage.getItem('mx_refresh_token')).toBe('replacement-refresh');
  });
});
