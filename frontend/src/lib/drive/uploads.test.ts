import { afterEach, beforeEach, expect, it, vi } from 'vitest';
const { fileUpload, batchUpload, folder, list, cancel } = vi.hoisted(()=>({fileUpload:vi.fn(),batchUpload:vi.fn(),folder:vi.fn(),list:vi.fn(),cancel:vi.fn()}));
vi.mock('../api/drive',()=>({uploadDriveFile:fileUpload,uploadDriveBatch:batchUpload,driveFolder:folder,driveList:list,driveCancelUpload:cancel}));
import { createDriveUploads } from './uploads.svelte';
import { ApiError, ConnectionError } from '../api/client';
import { readUploadRecovery } from './uploadRecovery';
beforeEach(()=>{
  fileUpload.mockReset();batchUpload.mockReset();folder.mockReset();list.mockReset();cancel.mockReset();
  const data=new Map<string,string>();
  vi.stubGlobal('localStorage',{get length(){return data.size;},key:(index:number)=>[...data.keys()][index]||null,getItem:(key:string)=>data.get(key)||null,setItem:(key:string,value:string)=>data.set(key,value),removeItem:(key:string)=>data.delete(key)});
  vi.stubGlobal('window',new EventTarget());vi.stubGlobal('navigator',{onLine:true});
});
afterEach(()=>{vi.useRealTimers();vi.unstubAllGlobals();});
const file=(name:string)=>Object.defineProperty(new File(['x'],name),'size',{value:2*1024**2});
const settle=async()=>{for(let i=0;i<10;i++)await Promise.resolve();};
it('keeps two-file scheduling bounded and resumes queued files without duplicate jobs',async()=>{
  const pending:{resolve:()=>void;signal:AbortSignal}[]=[];
  fileUpload.mockImplementation((_file,options)=>new Promise<void>(resolve=>pending.push({resolve,signal:options.signal})));
  const queue=createDriveUploads('owner'); await queue.enqueue([file('a'),file('b'),file('c')],null);
  expect(fileUpload).toHaveBeenCalledTimes(2); expect(queue.active()).toBe(true);
  queue.pause(queue.transfers[2]); expect(queue.transfers[2].status).toBe('paused');
  pending[0].resolve(); await settle(); expect(fileUpload).toHaveBeenCalledTimes(2);
  queue.resume(queue.transfers[2]); queue.resume(queue.transfers[2]); expect(fileUpload).toHaveBeenCalledTimes(3);
  pending[1].resolve(); pending[2].resolve(); await settle();
  expect(queue.transfers.every(item=>item.status==='done')).toBe(true); expect(queue.revision).toBe(3);
  expect(queue.active()).toBe(false); queue.clearCompleted(); expect(queue.transfers).toHaveLength(0);
  queue.dispose();
});
it('disposes on workspace exit and never starts another account\'s queued work',async()=>{
  fileUpload.mockImplementation((_file,options)=>new Promise((_resolve,reject)=>options.signal.addEventListener('abort',()=>reject(options.signal.reason))));
  const first=createDriveUploads('one'); await first.enqueue([file('a'),file('b'),file('c')],null);
  const signals=fileUpload.mock.calls.map(call=>call[1].signal); first.dispose(); await settle();
  expect(signals.every(signal=>signal.aborted)).toBe(true); expect(fileUpload).toHaveBeenCalledTimes(2); expect(first.transfers).toHaveLength(0);
  await first.enqueue([file('d')],null); expect(fileUpload).toHaveBeenCalledTimes(2);
  const second=createDriveUploads('two'); expect(second.transfers).toHaveLength(0); second.dispose();
});
it('pauses grouped small files together',async()=>{
  batchUpload.mockImplementation((_entries,_owner,signal)=>new Promise((_resolve,reject)=>signal.addEventListener('abort',()=>reject(signal.reason))));
  const queue=createDriveUploads('owner'); await queue.enqueue([new File(['a'],'a.txt'),new File(['b'],'b.txt')],null);
  queue.pause(queue.transfers[0]); await settle(); expect(queue.transfers.every(item=>item.status==='paused')).toBe(true);
  queue.dispose();
});
it('reports each partial batch result independently without losing the failed file',async()=>{
  batchUpload.mockResolvedValue([{status:200,item:{uid:'one'}},{status:409,error:'Name exists'}]);
  const queue=createDriveUploads('owner'); await queue.enqueue([new File(['a'],'a.txt'),new File(['b'],'b.txt')],null); await settle();
  expect(queue.transfers.map(item=>item.status)).toEqual(['done','failed']); expect(queue.transfers[1].error).toBe('Name exists');
  queue.clearCompleted(); expect(queue.transfers).toHaveLength(1); expect(queue.transfers[0].name).toBe('b.txt');
  batchUpload.mockResolvedValueOnce([{status:200,item:{uid:'two'}}]);
  queue.resume(queue.transfers[0]); await settle();
  expect(batchUpload).toHaveBeenCalledTimes(2); expect(batchUpload.mock.calls[1][0]).toHaveLength(1);
  expect(fileUpload).not.toHaveBeenCalled(); expect(queue.transfers[0].status).toBe('done'); queue.dispose();
});
it('creates dropped empty folders and respects explicit relative paths without copying File bytes',async()=>{
  list.mockResolvedValue({items:[]}); let n=0;folder.mockImplementation(()=>Promise.resolve({uid:`folder${++n}`}));
  fileUpload.mockResolvedValue({uid:'uploaded'});
  const original=file('one.bin');const queue=createDriveUploads('owner');
  await queue.enqueue([{file:original,relativePath:'References/nested/one.bin'}],null,undefined,['References/empty']); await settle();
  expect(folder.mock.calls.map(call=>[call[0],call[1]])).toEqual([['References',null],['empty','folder1'],['nested','folder1']]);
  expect(fileUpload.mock.calls[0][0]).toBe(original);expect(fileUpload.mock.calls[0][1].parentUid).toBe('folder3');queue.dispose();
});
it('keeps the file-manager clipboard account-scoped and clears it on signout',()=>{
  const first=createDriveUploads('one'),second=createDriveUploads('two');
  const item={uid:'item',name:'name',revision:1} as import('../api/drive').DriveItem;
  first.stage([item],'move');item.name='elsewhere';
  expect(first.clipboard?.items[0].name).toBe('name');expect(second.clipboard).toBeNull();
  first.dispose();expect(first.clipboard).toBeNull();second.dispose();
});

it('automatically retries transient failures with stable operation identity and no false ETA',async()=>{
  vi.useFakeTimers();
  fileUpload.mockImplementationOnce(async(_file,options)=>{options.onProgress({loaded:150,storedBytes:100,total:200,phase:'uploading',resumedParts:0,bytesPerSecond:50,etaSeconds:1});throw new ConnectionError('offline');}).mockResolvedValue({uid:'done'});
  const queue=createDriveUploads('owner');await queue.enqueue([file('large')],null);await settle();
  expect(queue.transfers[0].status).toBe('waiting');expect(queue.transfers[0].progress.loaded).toBe(100);
  expect(queue.transfers[0].progress.etaSeconds).toBeNull();expect(queue.transfers[0].progress.bytesPerSecond).toBe(0);
  await vi.advanceTimersByTimeAsync(2501);await settle();
  expect(queue.transfers[0].status).toBe('done');expect(fileUpload.mock.calls[1][1].operationUid).toBe(fileUpload.mock.calls[0][1].operationUid);
  queue.dispose();expect(vi.getTimerCount()).toBe(0);
});
it('waits offline, probes again on online, and manual pause prevents automatic resumption',async()=>{
  vi.useFakeTimers();
  fileUpload.mockImplementation((_file,options)=>new Promise((_resolve,reject)=>options.signal.addEventListener('abort',()=>reject(options.signal.reason))));
  const queue=createDriveUploads('owner');await queue.enqueue([file('large')],null);
  vi.stubGlobal('navigator',{onLine:false});window.dispatchEvent(new Event('offline'));await settle();
  expect(queue.transfers[0].status).toBe('waiting');await vi.advanceTimersByTimeAsync(60000);expect(fileUpload).toHaveBeenCalledTimes(1);
  queue.pause(queue.transfers[0]);vi.stubGlobal('navigator',{onLine:true});window.dispatchEvent(new Event('online'));await vi.advanceTimersByTimeAsync(60000);
  expect(queue.transfers[0].status).toBe('paused');expect(fileUpload).toHaveBeenCalledTimes(1);
  queue.resume(queue.transfers[0]);expect(fileUpload).toHaveBeenCalledTimes(2);queue.dispose();await settle();expect(vi.getTimerCount()).toBe(0);
});
it.each([401,403,409,413])('never auto-retries terminal HTTP %i even after online events',async(status)=>{
  vi.useFakeTimers();fileUpload.mockRejectedValue(new ApiError('denied',status,null));
  const queue=createDriveUploads('owner');await queue.enqueue([file('large')],null);await settle();
  expect(queue.transfers[0].status).toBe('failed');window.dispatchEvent(new Event('online'));await vi.advanceTimersByTimeAsync(60000);
  expect(fileUpload).toHaveBeenCalledTimes(1);queue.dispose();
});
it('recovers metadata after reload without retaining bytes, and rejects a different original file',async()=>{
  fileUpload.mockImplementation((_file,options)=>new Promise((_resolve,reject)=>options.signal.addEventListener('abort',()=>reject(options.signal.reason))));
  const original=file('original');const first=createDriveUploads('owner');await first.enqueue([original],null);
  const operation=first.transfers[0].operationUid;
  fileUpload.mock.calls[0][1].onOperation(crypto.randomUUID());const canonical=first.transfers[0].operationUid;expect(canonical).not.toBe(operation);
  first.dispose();await settle();
  const cached=readUploadRecovery('owner');expect('file' in cached[0]).toBe(false);expect(cached[0].operationUid).toBe(canonical);
  const next=createDriveUploads('owner'),other=createDriveUploads('another');expect(other.transfers).toHaveLength(0);
  expect(next.transfers[0].status).toBe('needs_file');expect(next.transfers[0].file).toBeNull();
  expect(next.reselect(next.transfers[0],file('wrong'))).toBe(false);expect(fileUpload).toHaveBeenCalledTimes(1);
  expect(next.reselect(next.transfers[0],original)).toBe(true);expect(fileUpload.mock.calls[1][1].operationUid).toBe(canonical);
  next.dispose();other.dispose();await settle();
});
it('retains manual pause after reload and merges reselected files into the existing queue entry',async()=>{
  fileUpload.mockImplementation((_file,options)=>new Promise((_resolve,reject)=>options.signal.addEventListener('abort',()=>reject(options.signal.reason))));
  const original=file('original');const first=createDriveUploads('owner');await first.enqueue([original],null);first.pause(first.transfers[0]);await settle();first.dispose();
  const next=createDriveUploads('owner');await next.enqueue([original],null);expect(next.transfers).toHaveLength(1);expect(next.transfers[0].status).toBe('paused');
  expect(fileUpload).toHaveBeenCalledTimes(1);next.resume(next.transfers[0]);expect(fileUpload).toHaveBeenCalledTimes(2);next.dispose();await settle();
});
it('keeps transient batch entries recoverable and does not replay acknowledged members',async()=>{
  vi.useFakeTimers();batchUpload.mockResolvedValueOnce([{status:200,item:{uid:'one'}},{status:503,error:'storage unavailable'}]).mockResolvedValue([{status:200,item:{uid:'two'}}]);
  const queue=createDriveUploads('owner');await queue.enqueue([new File(['a'],'a.txt'),new File(['b'],'b.txt')],null);await settle();
  expect(queue.transfers.map(entry=>entry.status)).toEqual(['done','waiting']);
  await vi.advanceTimersByTimeAsync(2501);expect(batchUpload.mock.calls[1][0]).toHaveLength(1);
  expect(batchUpload.mock.calls[1][0][0].operationUid).toBe(batchUpload.mock.calls[0][0][1].operationUid);queue.dispose();
});
it('removes completed recovery entries, exposes blocked storage, and cancels only the exact pending operation',async()=>{
  fileUpload.mockRejectedValue(new ApiError('conflict',409,null));cancel.mockResolvedValue({ok:true});
  const queue=createDriveUploads('owner');await queue.enqueue([file('large')],null);await settle();const operation=queue.transfers[0].operationUid;
  await queue.cancel(queue.transfers[0]);expect(cancel).toHaveBeenCalledWith(operation);expect(queue.transfers).toHaveLength(0);expect(readUploadRecovery('owner')).toHaveLength(0);queue.dispose();
  vi.stubGlobal('localStorage',{getItem(){throw Error('blocked');},setItem(){throw Error('blocked');},removeItem(){throw Error('blocked');}});
  fileUpload.mockResolvedValue({uid:'done'});const blocked=createDriveUploads('owner');await blocked.enqueue([file('large')],null);await settle();expect(blocked.recoveryAvailable).toBe(false);expect(blocked.transfers[0].status).toBe('done');blocked.dispose();
});

it('a second tab cannot erase another unfinished upload while writing or completing its own queue',async()=>{
  fileUpload.mockImplementation((_file,options)=>new Promise((_resolve,reject)=>options.signal.addEventListener('abort',()=>reject(options.signal.reason))));
  const first=createDriveUploads('owner');await first.enqueue([file('first')],null);
  const second=createDriveUploads('owner');await second.enqueue([file('second')],null);
  expect(readUploadRecovery('owner').map(item=>item.name).sort()).toEqual(['first','second']);
  first.dispose();second.dispose();await settle();expect(readUploadRecovery('owner')).toHaveLength(2);
});
it.each([new ApiError('Original file unreadable',422,null),new DOMException('Source unreadable','NotReadableError'),new DOMException('Source missing','NotFoundError')])('requests original bytes when a browser file handle becomes unreadable, without endless retry or lost progress (%s)',async(reason)=>{
  vi.useFakeTimers();fileUpload.mockImplementation(async(_file,options)=>{
    options.onProgress({loaded:150,storedBytes:100,total:200,phase:'uploading',resumedParts:0,bytesPerSecond:50,etaSeconds:1});
    throw reason;
  });
  const queue=createDriveUploads('owner');await queue.enqueue([file('large')],null);await settle();
  expect(queue.transfers[0].status).toBe('needs_file');expect(queue.transfers[0].file).toBeNull();
  expect(readUploadRecovery('owner')[0].storedBytes).toBe(100);await vi.advanceTimersByTimeAsync(60000);expect(fileUpload).toHaveBeenCalledTimes(1);queue.dispose();
});
