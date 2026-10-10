import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { describe, expect, it } from 'vitest';

const code=readFileSync(new URL('../../../public/office/offline-worker.js',import.meta.url),'utf8');
const origin='https://mx.test';
function harness(contents=new Map<string,Map<string,Response>>(), fetcher: (path:string)=>Promise<Response>=async()=>{throw new Error('MX unreachable');}) {
  const handlers=new Map<string,(event:Record<string,unknown>)=>void>();
  const caches={
    keys:async()=>[...contents.keys()],delete:async(name:string)=>contents.delete(name),
    open:async(name:string)=>{
      if(!contents.has(name))contents.set(name,new Map());
      const entries=contents.get(name)!;
      return {match:async(path:string)=>entries.get(path)?.clone(),put:async(path:string,response:Response)=>{entries.set(path,response.clone());}};
    }
  };
  const self={location:{origin},addEventListener:(name:string,handler:(event:Record<string,unknown>)=>void)=>handlers.set(name,handler),skipWaiting:async()=>{},clients:{claim:async()=>{}}};
  runInNewContext(code,{self,caches,fetch:fetcher,Response,URL});
  const lifecycle=async(name:string)=>{let work:Promise<unknown>|undefined;handlers.get(name)!({waitUntil:(promise:Promise<unknown>)=>work=promise});await work;};
  const request=async(path:string,method='GET')=>{let work:Promise<Response>|undefined;handlers.get('fetch')!({request:{url:new URL(path,origin).href,method},respondWith:(promise:Promise<Response>)=>work=promise});return work?await work:undefined;};
  return {contents,lifecycle,request};
}
describe('Office-only offline asset cache',()=>{
  it('cold activation offline retains the complete installed generation',async()=>{
    const manifest={cache:'mx-office-assets-installed',assets:['/office/index.html']};
    const contents=new Map([['mx-office-assets-installed',new Map([
      ['/office/precache.json',Response.json(manifest)],['/office/index.html',new Response('cached editor')]
    ])],['unrelated-cache',new Map<string,Response>()]]);
    const worker=harness(contents);
    await worker.lifecycle('activate');
    expect(await(await worker.request('/office/index.html?ignored=1'))!.text()).toBe('cached editor');
    expect(contents.has('unrelated-cache')).toBe(true);
  });
  it('an interrupted install never exposes a partial generation or removes the old editor',async()=>{
    const old=new Map([['/office/precache.json',Response.json({cache:'mx-office-assets-old'})],['/office/index.html',new Response('working editor')]]);
    const contents=new Map([['mx-office-assets-old',old]]);
    const worker=harness(contents,async path=>{
      if(path==='/office/precache.json')return Response.json({cache:'mx-office-assets-new',assets:['/office/index.html','/office/vendor/soffice.wasm']});
      if(path==='/office/index.html')return new Response('incomplete editor');
      return new Response('outage',{status:503});
    });
    await expect(worker.lifecycle('install')).rejects.toThrow('Office asset unavailable');
    expect(contents.has('mx-office-assets-new')).toBe(false);
    expect(await(await worker.request('/office/index.html'))!.text()).toBe('working editor');
  });
  it('activates a complete update and retires only the previous Office generation',async()=>{
    const contents=new Map([
      ['mx-office-assets-old',new Map([['/office/precache.json',Response.json({cache:'mx-office-assets-old'})],['/office/index.html',new Response('old editor')]])],
      ['unrelated-cache',new Map<string,Response>()]
    ]);
    const manifest={cache:'mx-office-assets-new',assets:['/office/index.html','/office/office-thread.js']};
    const worker=harness(contents,async path=>path==='/office/precache.json'?Response.json(manifest):new Response(`updated ${path}`));
    await worker.lifecycle('install');
    expect(contents.has('mx-office-assets-old')).toBe(true);
    await worker.lifecycle('activate');
    expect(contents.has('mx-office-assets-old')).toBe(false);
    expect(contents.has('unrelated-cache')).toBe(true);
    expect(await(await worker.request('/office/index.html'))!.text()).toBe('updated /office/index.html');
    expect(await(await worker.request('/office/office-thread.js'))!.text()).toBe('updated /office/office-thread.js');
  });
  it('does not intercept private APIs, root workspace, writes or foreign origins',async()=>{
    const worker=harness();
    for(const path of ['/mx/v1/drive','/mx/v1/auth/authenticate','/','https://foreign.test/office/index.html'])expect(await worker.request(path)).toBeUndefined();
    expect(await worker.request('/office/index.html','POST')).toBeUndefined();
  });
});
