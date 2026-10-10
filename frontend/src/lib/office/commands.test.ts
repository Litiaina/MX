import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { describe, expect, it } from 'vitest';

const adapter = readFileSync(new URL('../../../public/office/office-thread.js', import.meta.url), 'utf8');
type URL = { Complete: string };
type Interceptor = {
  queryDispatch: (url: URL, target: string, flags: number) => Dispatch;
  getInterceptedURLs: () => string[];
  setSlaveDispatchProvider: (provider: unknown) => void;
};
type Dispatch = { dispatch: (url: URL) => void; addStatusListener: (listener: unknown, url: URL) => void };
async function worker() {
  const messages: { cmd: string }[] = [];
  const operations: string[] = [];
  const releases: Interceptor[] = [];
  const providers = ['desktop', 'frame'].map(name => ({
    interceptor: null as Interceptor | null,
    registerDispatchProviderInterceptor(interceptor: Interceptor) {
      this.interceptor = interceptor;
      interceptor.setSlaveDispatchProvider({ queryDispatch: () => ({ provider: name }) });
    },
    releaseDispatchProviderInterceptor(interceptor: Interceptor) { releases.push(interceptor); },
    getContainerWindow: () => ({}), getComponentWindow: () => ({ setFocus() {} }), activate(){operations.push('activate-frame');}, contextChanged() {}
  }));
  const [desktop, frame] = providers;
  let modifyListener: {modified:()=>void};
  const selection={getRangeAddress:()=>({StartColumn:3,StartRow:0})};
  const model = { getCurrentController: () => ({ getFrame: () => frame,getSelection:()=>selection,select:(value:unknown)=>{expect(value).toBe(selection);operations.push('restore-selection');},addSelectionChangeListener(){},removeSelectionChangeListener(){} }), addModifyListener(value:{modified:()=>void}) {modifyListener=value;}, removeModifyListener() {}, isModified:()=>true,
    setModified(value:boolean) {operations.push(`modified:${value}`);}, storeToURL(){operations.push('export');}, close() {} };
  const port = { onmessage: null as null | ((event: { data: unknown }) => void), postMessage(message: { cmd: string }) { messages.push(message); } };
  class Properties { constructor(value: object) { Object.assign(this, value); } }
  const zeta = { uno: { com: { sun: { star: { frame: { Desktop: { create: () => ({ ...desktop, loadComponentFromURL: () => model }) },
    DispatchHelper:{create:()=>({executeDispatch:(_frame:unknown,url:string,_target:string,_flags:number,args:{Name:string;Value:unknown}[])=>{expect(args).toEqual([{Name:'SynchronMode',Value:true}]);operations.push(url);modifyListener.modified();}})},FeatureStateEvent: Properties }, beans: { PropertyValue: Properties }, awt: {Toolkit:{create:()=>({reschedule(){operations.push('flush-events');},processEventsToIdle(){operations.push('flush-idle');}})}} } } } },
    getUnoComponentContext: () => ({}),
    unoObject: (_interfaces: string[], value: object) => value, mainPort: port };
  // Keep the provider identity used by Desktop.create for inspection.
  const root = { ...desktop, loadComponentFromURL: () => model, getCurrentFrame:()=>null };
  zeta.uno.com.sun.star.frame.Desktop.create = () => root;
  runInNewContext(adapter, { Module: { zetajs: Promise.resolve(zeta) }, console: { info() {} } });
  await Promise.resolve();
  const open = (readOnly = false) => port.onmessage!({ data: { cmd: 'open', id: 1, extension: '.xlsx', readOnly } });
  const request = (data:unknown) => port.onmessage!({data});
  return { root, frame, messages, releases, open, request, operations };
}
describe('Office native file command boundary', () => {
  it('intercepts desktop-targeted Open before any document is loaded', async () => {
    const { root, messages } = await worker();
    const url = { Complete: '.uno:Open' };
    root.interceptor!.queryDispatch(url, '_blank', 0).dispatch(url);
    expect(messages.at(-1)?.cmd).toBe('open-request');
  });
  it('routes frame/root Open, remote Open and recent file URLs without invoking a native picker', async () => {
    const { root, frame, messages, open } = await worker(); open();
    for (const provider of [root, frame]) for (const name of ['.uno:Open', '.uno:OpenRemote', '.uno:OpenFromCalc', '.uno:OpenFromWriter', '.uno:OpenFromDraw', '.uno:OpenFromImpress', 'file:///tmp/mx-office/document.xlsx']) {
      const url = { Complete: name }; provider.interceptor!.queryDispatch(url, '_blank', 0).dispatch(url);
      expect(messages.at(-1)?.cmd).toBe('open-request');
      let enabled=false;
      provider.interceptor!.queryDispatch(url,'_blank',0).addStatusListener({statusChanged:(status:{IsEnabled:boolean})=>enabled=status.IsEnabled},url);
      expect(enabled).toBe(true);
    }
  });
  it('keeps independent forwarding chains and releases only the old document interceptor', async () => {
    const { root, frame, releases, open } = await worker(); open();
    const url = { Complete: '.uno:Bold' };
    expect(root.interceptor!.queryDispatch(url, '', 0)).toEqual({ provider: 'desktop' });
    expect(frame.interceptor!.queryDispatch(url, '', 0)).toEqual({ provider: 'frame' });
    const oldFrame = frame.interceptor; const rootInterceptor = root.interceptor;
    open(); expect(releases).toEqual([oldFrame]); expect(root.interceptor).toBe(rootInterceptor);
  });
  it('keeps Save routed to MX and disables publishing for read-only documents', async () => {
    const { frame, messages, open } = await worker(); open(true);
    const url = { Complete: '.uno:Save' }; const dispatch = frame.interceptor!.queryDispatch(url, '', 0);
    let enabled = true;
    dispatch.addStatusListener({ statusChanged: (status: { IsEnabled: boolean }) => { enabled = status.IsEnabled; } }, url);
    expect(enabled).toBe(false);
    dispatch.dispatch(url); expect(messages.at(-1)?.cmd).toBe('save-request');
    open(false);
    frame.interceptor!.queryDispatch({ Complete: '.uno:SaveAs' }, '', 0).dispatch({ Complete: '.uno:SaveAs' });
    expect(messages.at(-1)?.cmd).toBe('save-copy-request');
  });
  it('accepts current Calc cell input before exporting and only cleans an acknowledged matching generation', async () => {
    const {open, request, messages, operations}=await worker();open();
    operations.length=0;
    request({cmd:'export',id:2,extension:'.xlsx',filter:'Calc MS Excel 2007 XML'});
    expect(operations).toEqual(['.uno:AcceptFormula','restore-selection','export']);
    expect(messages.at(-1)).toMatchObject({cmd:'exported',generation:1});
    request({cmd:'committed',generation:0});expect(operations).not.toContain('modified:false');
    request({cmd:'committed',generation:1});expect(operations.at(-1)).toBe('modified:false');
  });
  it('acknowledges native document focus instead of assuming HTML canvas focus is sufficient', async () => {
    const {open,request,messages,operations}=await worker();open();request({cmd:'focus',id:77});
    expect(operations).toEqual(['activate-frame','restore-selection','activate-frame']);
    expect(messages.at(-1)).toMatchObject({cmd:'focused',id:77});
  });
  it('initializes the native Calc input selection before announcing the document as opened', async () => {
    const {open,messages,operations}=await worker();open();
    expect(operations).toEqual(['activate-frame','restore-selection']);
    expect(messages).toContainEqual({cmd:'selection',cell:'D1'});
    expect(messages.at(-1)).toMatchObject({cmd:'opened',id:1,generation:0});
  });
});
