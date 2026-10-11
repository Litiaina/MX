import { afterEach, expect, it, vi } from 'vitest';
import { readUploadRecovery, writeUploadRecovery, type UploadRecovery } from './uploadRecovery';
afterEach(()=>vi.unstubAllGlobals());
it('ignores malformed entries independently and validates recovery metadata without reading files',()=>{
  const prefix='mx:drive:pending:owner:',uid=crypto.randomUUID();
  const entry:UploadRecovery={uid,operationUid:crypto.randomUUID(),name:'large.mp4',size:20*1024**3,lastModified:1,mimeType:'video/mp4',relativePath:'large.mp4',parent:null,storedBytes:8*1024**3,manuallyPaused:false};
  const data=new Map<string,string>();vi.stubGlobal('localStorage',{get length(){return data.size;},key:(i:number)=>[...data.keys()][i]||null,getItem:(key:string)=>data.get(key)||null,setItem:(key:string,value:string)=>data.set(key,value),removeItem:(key:string)=>data.delete(key)});
  writeUploadRecovery('owner',[entry]);data.set(`${prefix}bad`,'{invalid');data.set(`${prefix}unsafe`,JSON.stringify({...entry,size:Number.MAX_SAFE_INTEGER+1}));
  expect(readUploadRecovery('owner')).toEqual([entry]);expect(readUploadRecovery('other')).toEqual([]);
  writeUploadRecovery('owner',[],[uid]);expect(readUploadRecovery('owner')).toEqual([]);
});
it('retains recovery entries across multiple batches instead of hiding entries after the first thousand',()=>{
  const data=new Map<string,string>();vi.stubGlobal('localStorage',{get length(){return data.size;},key:(i:number)=>[...data.keys()][i]||null,getItem:(key:string)=>data.get(key)||null,setItem:(key:string,value:string)=>data.set(key,value),removeItem:(key:string)=>data.delete(key)});
  const entries=Array.from({length:1001},(_,i):UploadRecovery=>({uid:crypto.randomUUID(),operationUid:crypto.randomUUID(),name:`file-${i}`,size:1,lastModified:1,mimeType:'text/plain',relativePath:`file-${i}`,parent:null,storedBytes:0,manuallyPaused:false}));
  expect(writeUploadRecovery('owner',entries)).toBe(true);expect(readUploadRecovery('owner')).toHaveLength(1001);
});
