import { afterEach, describe, expect, it, vi } from 'vitest';
import { OfficeEngine } from './engine';
vi.mock('./capabilities',()=>({checkOfficeGraphics:async()=>{}}));
vi.mock('./compatibility',()=>({installOfficeCompatibility:()=>()=>{}}));
afterEach(()=>{vi.useRealTimers();vi.unstubAllGlobals();});
function setup(ready=true){
  vi.useFakeTimers();
  const port={onmessage:undefined as undefined|((event:{data:unknown})=>void),postMessage:vi.fn(),close:vi.fn()};
  const terminate=vi.fn();let resolvePort!:(port:unknown)=>void;
  const win={Module:{} as Record<string,unknown>,FS:{mkdir:vi.fn(),writeFile:vi.fn()},PThread:{terminateAllThreads:terminate},addEventListener:vi.fn(),removeEventListener:vi.fn(),dispatchEvent:vi.fn()};
  vi.stubGlobal('window',win);vi.stubGlobal('isSecureContext',true);vi.stubGlobal('crossOriginIsolated',true);vi.stubGlobal('location',{origin:'https://mx.test'});
  vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});
  vi.stubGlobal('document',{createElement:()=>({}),body:{append:(script:{onload:()=>void})=>{
    win.Module.uno_main=ready?Promise.resolve(port):new Promise(resolve=>resolvePort=resolve);script.onload();
    if(ready)queueMicrotask(()=>port.onmessage?.({data:{cmd:'ready'}}));
  }}});
  const engine=new OfficeEngine(vi.fn(),vi.fn(),vi.fn(),vi.fn());
  return {engine,port,terminate,resolve:()=>resolvePort(port)};
}
describe('Office long operation recovery',()=>{
  it('does not reject a real opening acknowledgement after the former 60-second deadline',async()=>{
    const {engine,port}=setup();await engine.start({} as HTMLCanvasElement);
    const opening=engine.open(new Blob(['workbook']),'.xlsx',false);await vi.advanceTimersByTimeAsync(61000);
    const request=port.postMessage.mock.calls[0][0];port.onmessage!({data:{cmd:'opened',id:request.id}});
    const prepared=await opening;expect(prepared.generation).toBe(0);expect(await prepared.blob.text()).toBe('workbook');engine.dispose();
  });
  it('terminates worker computation when opening is cancelled and rejects its promise',async()=>{
    const {engine,terminate}=setup();await engine.start({} as HTMLCanvasElement);
    const opening=engine.open(new Blob(['workbook']),'.xlsx',false);const rejected=expect(opening).rejects.toThrow('cancelled');
    await vi.advanceTimersByTimeAsync(1);engine.dispose();await rejected;expect(terminate).toHaveBeenCalled();
  });
  it('ignores and closes a channel that arrives after startup cancellation',async()=>{
    const {engine,port,resolve,terminate}=setup(false);const starting=engine.start({} as HTMLCanvasElement);
    const rejected=expect(starting).rejects.toThrow('cancelled');await vi.advanceTimersByTimeAsync(1);engine.dispose();await rejected;
    resolve();await vi.advanceTimersByTimeAsync(1);expect(port.close).toHaveBeenCalled();expect(terminate).toHaveBeenCalled();
  });
});
