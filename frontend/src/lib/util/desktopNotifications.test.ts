import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {DesktopNotifications,desktopCapability,desktopExplanation,lastDesktopAttempt} from './desktopNotifications';
describe('Desktop notification delivery policy',()=>{
  let focused:boolean,visible:string,permission:string,fail:boolean;
  let notices:Array<{title:string;options:NotificationOptions;close:ReturnType<typeof vi.fn>}>;
  let storage:Map<string,string>;
  beforeEach(()=>{
    focused=true;visible='visible';permission='granted';fail=false;notices=[];storage=new Map();
    const document=new EventTarget();Object.assign(document,{hasFocus:()=>focused});Object.defineProperty(document,'visibilityState',{get:()=>visible});
    vi.stubGlobal('document',document);
    vi.stubGlobal('window',Object.assign(new EventTarget(),{isSecureContext:true,focus:vi.fn()}));
    class Notice{static get permission(){return permission;}close=vi.fn();constructor(title:string,options:NotificationOptions){if(fail)throw new Error('Browser delivery rejected');notices.push({title,options,close:this.close});}}
    vi.stubGlobal('Notification',Notice);Object.assign(window,{Notification:Notice});
    vi.stubGlobal('localStorage',{getItem:(key:string)=>storage.get(key)??null,setItem:(key:string,value:string)=>storage.set(key,value),removeItem:(key:string)=>storage.delete(key)});
    vi.stubGlobal('navigator',{locks:{request:async(_key:string,callback:()=>Promise<unknown>)=>callback()}});
  });
  afterEach(()=>vi.unstubAllGlobals());
  const alert=(uid='item-1')=>({uid,title:'Record updated',body:'A record changed',onOpen:vi.fn()});
  it('keeps OS silent in focused MX and still allows in-app sound',async()=>{
    const delivery=new DesktopNotifications('alice');const sound=vi.fn();
    expect(await delivery.deliver(alert(),true,sound)).toBe('focused');expect(sound).toHaveBeenCalledOnce();expect(notices).toHaveLength(0);delivery.stop();
  });
  it('delivers for a minimized window or another focused app without waiting on a timer',async()=>{
    focused=false;visible='hidden';const delivery=new DesktopNotifications('alice');
    expect(await delivery.deliver(alert(),true)).toBe('submitted');expect(notices[0].options.silent).toBe(true);
    visible='visible';expect(await delivery.deliver(alert('item-2'),true)).toBe('submitted');expect(notices).toHaveLength(2);delivery.stop();
  });
  it('does not turn a foreground alert into an OS popup while awaiting a receipt',async()=>{
    let release:()=>void=()=>{};
    vi.stubGlobal('navigator',{locks:{request:async(_key:string,callback:()=>Promise<unknown>)=>{await new Promise<void>(resolve=>{release=resolve;});return callback();}}});
    const delivery=new DesktopNotifications('alice');const pending=delivery.deliver(alert(),true);
    focused=false;visible='hidden';release();
    expect(await pending).toBe('focused');expect(notices).toHaveLength(0);delivery.stop();
  });
  it('does not popup from a background tab when another MX tab is focused',async()=>{
    focused=false;storage.set('mx-focus:alice',JSON.stringify({tab:'other-tab',at:Date.now()}));
    const delivery=new DesktopNotifications('alice');expect(await delivery.deliver(alert(),true)).toBe('focused');expect(notices).toHaveLength(0);delivery.stop();
  });
  it('keeps the actual last attempt across focus changes, scoped to the account',async()=>{
    focused=false;fail=true;
    const delivery=new DesktopNotifications('diagnostic-account');delivery.start();
    expect(await delivery.deliver(alert(),true)).toBe('failed');
    const result=lastDesktopAttempt('diagnostic-account');expect(result?.status).toBe('failed');
    focused=true;window.dispatchEvent(new Event('focus'));expect(lastDesktopAttempt('diagnostic-account')).toEqual(result);
    expect(lastDesktopAttempt('another-diagnostic-account')).toBeUndefined();delivery.stop();
  });
  it('reports cross-tab focus suppression rather than leaving a test queued forever',async()=>{
    focused=false;storage.set('mx-focus:diagnostic-suppressed',JSON.stringify({tab:'other-tab',at:Date.now()}));
    const delivery=new DesktopNotifications('diagnostic-suppressed');const status=vi.fn();window.addEventListener('mx-desktop-status',status);
    expect(await delivery.deliver(alert(),true)).toBe('focused');expect(status).toHaveBeenCalledOnce();expect(lastDesktopAttempt('diagnostic-suppressed')?.status).toBe('focused');delivery.stop();
  });
  it('de-duplicates reconnect replay across instances but not across accounts',async()=>{
    focused=false;const a=new DesktopNotifications('alice'),b=new DesktopNotifications('alice'),c=new DesktopNotifications('bob');
    expect(await a.deliver(alert(),true)).toBe('submitted');expect(await b.deliver(alert(),true)).toBe('duplicate');expect(await c.deliver(alert(),true)).toBe('submitted');expect(notices).toHaveLength(2);a.stop();b.stop();c.stop();
  });
  it('handles blocked permission, insecure origins, unsupported API and constructor rejection honestly',async()=>{
    focused=false;const delivery=new DesktopNotifications('alice');
    permission='denied';expect(await delivery.deliver(alert('denied'),true)).toBe('denied');
    permission='default';expect(desktopCapability()).toBe('permission-needed');
    window.isSecureContext=false;expect(desktopCapability()).toBe('insecure');window.isSecureContext=true;
    permission='granted';fail=true;expect(await delivery.deliver(alert('failed'),true)).toBe('failed');
    expect(desktopExplanation('submitted')).toContain('operating system');delete (window as unknown as {Notification?:unknown}).Notification;expect(desktopCapability()).toBe('unsupported');delivery.stop();
  });
  it('closes current banners when MX regains focus and never delivers after disposal',async()=>{
    focused=false;const delivery=new DesktopNotifications('alice');delivery.start();await delivery.deliver(alert(),true);
    focused=true;window.dispatchEvent(new Event('focus'));expect(notices[0].close).toHaveBeenCalledOnce();delivery.stop();expect(await delivery.deliver(alert('later'),true)).toBe('disabled');
  });
});
