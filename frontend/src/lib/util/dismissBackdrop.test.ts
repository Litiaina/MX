import {describe,expect,it,vi} from 'vitest';
import {dismissBackdrop} from './dismissBackdrop';

function fixture() {
  const listeners=new Map<string,(event:PointerEvent)=>void>();
  const node={addEventListener:(name:string,handler:(event:PointerEvent)=>void)=>listeners.set(name,handler),removeEventListener:(name:string)=>listeners.delete(name)} as unknown as HTMLElement;
  const close=vi.fn(), action=dismissBackdrop(node,close);
  const event=(target:unknown=node,button=0,pointerId=1)=>({target,button,pointerId,preventDefault:vi.fn()}) as unknown as PointerEvent;
  const send=(name:string,value=event())=>{listeners.get(name)?.(value);return value;};
  return {node,close,action,event,send,listeners};
}
describe('safe backdrop dismissal',()=>{
  it('dismisses a primary backdrop gesture without stealing focus before confirmation',()=>{
    const f=fixture();const down=f.send('pointerdown');expect(down.preventDefault).toHaveBeenCalledOnce();
    f.send('pointerup');expect(f.close).toHaveBeenCalledOnce();
  });
  it('does not dismiss an inside-to-outside drag, another pointer or a canceled gesture',()=>{
    const f=fixture();f.send('pointerdown',f.event({}));f.send('pointerup');
    f.send('pointerdown');f.send('pointerup',f.event(f.node,0,2));
    f.send('pointerdown');f.send('pointercancel');f.send('pointerup');
    f.send('pointerdown',f.event(f.node,2));f.send('pointerup');
    expect(f.close).not.toHaveBeenCalled();
  });
  it('uses the current callback and removes listeners when the dialog is destroyed',()=>{
    const f=fixture(),next=vi.fn();f.action.update(next);f.send('pointerdown');f.send('pointerup');
    expect(f.close).not.toHaveBeenCalled();expect(next).toHaveBeenCalledOnce();
    f.action.destroy();expect(f.listeners.size).toBe(0);
  });
});
