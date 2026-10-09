import { afterEach, beforeEach, expect, it, vi } from 'vitest';
const { json, upload, part } = vi.hoisted(() => ({ json: vi.fn(), upload: vi.fn(), part: vi.fn() }));
vi.mock('./client', () => ({ apiJson: json, apiUpload: upload, apiUploadPart: part, apiFile: vi.fn() }));
import { uploadDriveFile, uploadDriveBatch, driveList, driveNeighbors, driveSpaceGrant, guestDrive, guestNeighbors } from './drive';
const options = () => ({ accountUid: crypto.randomUUID(), parentUid: null });
beforeEach(() => { json.mockReset(); upload.mockReset(); part.mockReset(); part.mockResolvedValue({stored:true}); });
afterEach(() => vi.unstubAllGlobals());

it('requests bounded pages with server-side sorting/search and folder filters',async()=>{
  json.mockResolvedValue({items:[]});await driveList('shared','folder','Quarterly & annual',100,{limit:25,sort:'name_asc',kind:'folder'});
  const url=new URL(json.mock.calls[0][0],'https://mx.test');expect(Object.fromEntries(url.searchParams)).toEqual({view:'shared',parent_uid:'folder',q:'Quarterly & annual',offset:'100',limit:'25',sort:'name_asc',kind:'folder'});
});
it('keeps preview neighbors in the same listing and cancels stale navigation',async()=>{
  const controller=new AbortController();await driveNeighbors('file','starred','folder','report','modified_asc',controller.signal);
  expect(json.mock.calls[0][0]).toContain('/items/file/neighbors?');expect(json.mock.calls[0][0]).toContain('view=starred');expect(json.mock.calls[0][0]).toContain('sort=modified_asc');expect(json.mock.calls[0][1].signal).toBe(controller.signal);
});
it('sends a space grant without confusing it with a user grant',async()=>{
  await driveSpaceGrant('file','space','editor');expect(JSON.parse(json.mock.calls[0][1].body)).toEqual({space_uid:'space',role:'editor'});expect(json.mock.calls[0][1].retrySafe).toBe(true);
});
it('guest pages and neighbors stay unauthenticated and within the secret link',async()=>{
  await guestDrive('secret/token','folder',50,25);expect(json.mock.calls[0][0]).toContain('/guest/secret%2Ftoken?');expect(json.mock.calls[0][0]).toContain('offset=50');expect(json.mock.calls[0][0]).toContain('limit=25');expect(json.mock.calls[0][2]).toBe(false);
  const controller=new AbortController();await guestNeighbors('secret/token','file',controller.signal);expect(json.mock.calls[1][0]).toContain('/guest/secret%2Ftoken/neighbors?item_uid=file');expect(json.mock.calls[1][1].signal).toBe(controller.signal);expect(json.mock.calls[1][2]).toBe(false);
});

it('retains retry identity even when browser storage is unavailable', async () => {
  vi.stubGlobal('localStorage', { getItem() { throw new Error('blocked'); }, setItem() { throw new Error('blocked'); } });
  const file = new File(['content'], 'Original.MP4'); const scope = options();
  json.mockRejectedValueOnce(new Error('lost init response'));
  await expect(uploadDriveFile(file, scope)).rejects.toThrow('lost init response');
  const first = JSON.parse(json.mock.calls[0][1].body);
  json.mockResolvedValueOnce({ completed: true, item: { uid: 'saved' } });
  expect(await uploadDriveFile(file, scope)).toEqual({ uid: 'saved' });
  const second = JSON.parse(json.mock.calls[1][1].body);
  expect(second).toEqual(first);
  expect(first.sha256).toBeUndefined();
  expect(first.last_modified).toBe(file.lastModified);
});

it('uploads only missing parts and finalizes the same operation', async () => {
  const file = new File(['abcdefghij'], 'resume.txt'); const progress = vi.fn();
  json.mockImplementation(async (path: string) => path.endsWith('/uploads') ? { completed: false, part_size: 4, uploaded_parts: [0, 2] } : path.endsWith('/finish') ? { uid: 'saved' } : { stored: true });
  await uploadDriveFile(file, { ...options(), onProgress: progress });
  const init = JSON.parse(json.mock.calls[0][1].body);
  const parts = part.mock.calls;
  expect(parts).toHaveLength(1);
  expect(parts[0][0]).toBe(`/mx/v1/drive/uploads/${init.operation_uid}/parts/1`);
  expect(await parts[0][1].text()).toBe('efgh');
  expect(json.mock.calls.at(-1)?.[0]).toBe(`/mx/v1/drive/uploads/${init.operation_uid}/finish`);
  expect(progress.mock.calls.some(([value]) => value.loaded === 6 && value.resumedParts === 2)).toBe(true);
});

it('reconciles an already finalized N1 object without sending more parts', async () => {
  json.mockResolvedValueOnce({ part_size: 16, ready_to_finalize: true }).mockResolvedValueOnce({ uid: 'saved' });
  await uploadDriveFile(new File(['saved bytes'], 'lost.txt'), options());
  expect(json).toHaveBeenCalledTimes(2);
  expect(json.mock.calls[1][0]).toMatch(/\/finish$/);
});

it('does not finalize when a part failed or the upload was canceled', async () => {
  json.mockResolvedValueOnce({ part_size: 16 }); part.mockRejectedValueOnce(new Error('offline'));
  await expect(uploadDriveFile(new File(['bytes'], 'failed.txt'), options())).rejects.toThrow('offline');
  expect(json.mock.calls.some(([path]) => path.endsWith('/finish'))).toBe(false);
  const controller = new AbortController(); controller.abort(); json.mockClear();
  await expect(uploadDriveFile(new File(['bytes'], 'aborted.txt'), { ...options(), signal: controller.signal })).rejects.toThrow();
  expect(json).not.toHaveBeenCalled();
});

it('matches partial batch results by stable operation rather than response order', async () => {
  const files = [new File(['one'], 'one.txt'), new File(['two'], 'two.txt')]; const scope = options();
  upload.mockImplementation(async (_path, body: FormData) => {
    const metadata = JSON.parse(String(body.get('metadata')));
    expect(body.getAll('file')).toHaveLength(2);
    return { results: [{ operation_uid: metadata[1].operation_uid, status: 409, error: 'Name exists' }, { operation_uid: metadata[0].operation_uid, status: 200, item: { uid: 'one' } }] };
  });
  const results = await uploadDriveBatch(files.map((file) => ({ file, parentUid: null })), scope.accountUid, new AbortController().signal);
  expect(results.map((value) => value.status)).toEqual([200, 409]);
  await uploadDriveBatch(files.map((file) => ({ file, parentUid: null })), scope.accountUid, new AbortController().signal);
  const first=JSON.parse(String(upload.mock.calls[0][1].get('metadata'))); const second=JSON.parse(String(upload.mock.calls[1][1].get('metadata')));
  expect(second[1].operation_uid).toEqual(first[1].operation_uid); // failed entry retains replay identity
  expect(second[0].operation_uid).not.toEqual(first[0].operation_uid); // acknowledged new selection is a new operation
});

it('accepts console 50 MiB parts instead of imposing the former 32 MiB part ceiling', async () => {
  json.mockResolvedValueOnce({ part_size: 50*1024*1024, workers: 4 }).mockResolvedValueOnce({uid:'saved'});
  expect(await uploadDriveFile(new File(['small last part'],'large.MP4'),options())).toEqual({uid:'saved'});
});

it('rejects invalid server part metadata and stops scheduling after a part fails',async()=>{
  json.mockResolvedValueOnce({part_size:50*1024*1024+1});
  await expect(uploadDriveFile(new File(['bytes'],'invalid.txt'),options())).rejects.toThrow('invalid multipart size');
  json.mockReset();
  part.mockRejectedValue(new Error('offline'));
  json.mockImplementation(async(path:string)=>path.endsWith('/uploads')?{part_size:1,workers:2}:Promise.reject(new Error('offline')));
  await expect(uploadDriveFile(new File(['abcdefghijklmnop'],'interrupted.txt'),options())).rejects.toThrow('offline');
  expect(part.mock.calls.length).toBeLessThanOrEqual(2);
  expect(json.mock.calls.some(([path])=>path.endsWith('/finish'))).toBe(false);
});

it('starts a virtual 15 GB file without reading or hashing any file bytes', async () => {
  const scope = options(); const slice = vi.fn(() => new Blob(['bounded part']));
  const read = vi.fn(() => { throw new Error('Full-file read forbidden'); });
  const file = { name:'15GB.mp4', size:15*1024**3, type:'video/mp4', lastModified:1234, slice, arrayBuffer:read, stream:read } as unknown as File;
  json.mockResolvedValueOnce({part_size:50*1024**2,workers:4,operation_uid:'server-resumed'});
  part.mockRejectedValue(new Error('test stops after initial parts'));
  await expect(uploadDriveFile(file,scope)).rejects.toThrow('test stops');
  expect(read).not.toHaveBeenCalled(); expect(slice).toHaveBeenCalledTimes(4);
  expect(slice.mock.calls[0]).toEqual([0,50*1024**2]);
  expect(part.mock.calls[0][0]).toContain('/server-resumed/parts/0');
  const metadata = JSON.parse(json.mock.calls[0][1].body);
  expect(metadata.size).toBe(15*1024**3); expect(metadata.sha256).toBeUndefined();
});

it('checks quota before slicing bytes and distinguishes changed modification time',async()=>{
  const file=new File(['bytes'],'same.txt',{lastModified:1}); const slice=vi.spyOn(file,'slice'); const scope=options();
  json.mockRejectedValueOnce(new Error('quota exceeded'));
  await expect(uploadDriveFile(file,scope)).rejects.toThrow('quota'); expect(slice).not.toHaveBeenCalled();
  const first=JSON.parse(json.mock.calls[0][1].body).operation_uid;
  json.mockRejectedValueOnce(new Error('quota exceeded'));
  await expect(uploadDriveFile(new File(['bytes'],'same.txt',{lastModified:2}),scope)).rejects.toThrow('quota');
  expect(JSON.parse(json.mock.calls[1][1].body).operation_uid).not.toBe(first);
});
