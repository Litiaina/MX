import {beforeEach, expect, it, vi} from 'vitest';
const {request}=vi.hoisted(()=>({request:vi.fn()}));
vi.mock('./client',()=>({apiJson:request,apiFile:vi.fn(),apiUpload:vi.fn(),jsonRequest:vi.fn()}));
import {createBackup,listBackups} from './workspace';
beforeEach(()=>request.mockReset());
it('allows long backup creation without replaying an uncertain write',async()=>{
  await createBackup();
  const [path,options]=request.mock.calls[0];
  expect(path).toBe('/mx/v1/admin/backups');
  expect(options).toEqual({method:'POST',timeoutMs:30*60_000,maxAttempts:1});
  expect(options.retrySafe).not.toBe(true);
});
it('keeps status checks cancellable and bounded; polling controls retries',async()=>{
  const controller=new AbortController();await listBackups(controller.signal);
  expect(request.mock.calls[0]).toEqual(['/mx/v1/admin/backups',{signal:controller.signal,timeoutMs:10_000,maxAttempts:1}]);
});
