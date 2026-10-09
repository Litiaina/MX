import { beforeEach, expect, it, vi } from 'vitest';
const { fileUpload, batchUpload, folder, list } = vi.hoisted(()=>({fileUpload:vi.fn(),batchUpload:vi.fn(),folder:vi.fn(),list:vi.fn()}));
vi.mock('../api/drive',()=>({uploadDriveFile:fileUpload,uploadDriveBatch:batchUpload,driveFolder:folder,driveList:list}));
import { createDriveUploads } from './uploads.svelte';
beforeEach(()=>{fileUpload.mockReset();batchUpload.mockReset();folder.mockReset();list.mockReset();});
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
  queue.clearCompleted(); expect(queue.transfers).toHaveLength(1); expect(queue.transfers[0].file.name).toBe('b.txt');
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
