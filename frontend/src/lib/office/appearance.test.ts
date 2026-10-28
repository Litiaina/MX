import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {initializeOfficeAppearance,rememberOfficeAppearance} from './appearance';
import {apiJson} from '../api/client';
import {applyBrandIcon} from '../util/branding';
vi.mock('../api/client',()=>({apiJson:vi.fn(),hasAuthTokens:()=>true}));
vi.mock('../util/branding',()=>({applyBrandIcon:vi.fn()}));
let storage:Map<string,string>,style:Map<string,string>,root:{dataset:Record<string,string>;style:Record<string,unknown>};
const config={branding:{logo_url:'images/org.png'},appearance:{preset:'sage',primary_color:'#c94128',default_theme:'light'}};
beforeEach(()=>{
  vi.clearAllMocks();storage=new Map();style=new Map();
  root={dataset:{},style:{fontSize:'',setProperty:(key:string,value:string)=>style.set(key,value),removeProperty:(key:string)=>style.delete(key),getPropertyValue:(key:string)=>style.get(key)||''}};
  vi.stubGlobal('document',{documentElement:root,querySelector:()=>({href:'https://mx.example/images/org.png'})});
  vi.stubGlobal('sessionStorage',{getItem:(key:string)=>storage.get(key)??null,setItem:(key:string,value:string)=>storage.set(key,value)});
  vi.mocked(apiJson).mockRejectedValue(new Error('Offline'));
});
afterEach(()=>vi.unstubAllGlobals());
describe('Office appearance without delaying the engine',()=>{
  it('applies a copied workspace snapshot immediately and keeps it offline',async()=>{
    storage.set('mx_office_appearance',JSON.stringify({theme:'dark',accent:'#33aaff',fontPercent:125,reducedMotion:true,logo:'images/org.png'}));
    initializeOfficeAppearance();expect(root.dataset.theme).toBe('dark');expect(root.style.fontSize).toBe('125%');expect(style.get('--primary')).toBe('#33aaff');expect(applyBrandIcon).toHaveBeenCalledWith('images/org.png');
    await Promise.resolve();await Promise.resolve();expect(root.dataset.theme).toBe('dark');
  });
  it('refreshes deployment branding and personal preferences in a direct Office tab',async()=>{
    vi.mocked(apiJson).mockImplementation(async path=>path.includes('/deployment/')?{config}:{preferences:{theme:'dark',accent_color:'#ffffff',font_size_px:24,reduced_motion:true}});
    initializeOfficeAppearance();await vi.waitFor(()=>expect(root.style.fontSize).toBe('150%'));
    expect(root.dataset.theme).toBe('dark');expect(style.get('--primary')).toBe('#ffffff');expect(style.get('--on-primary')).toBe('#0f172a');expect(applyBrandIcon).toHaveBeenLastCalledWith('images/org.png');
  });
  it('keeps the adaptive product accent unset instead of pinning the light-mode color',async()=>{
    vi.mocked(apiJson).mockImplementation(async path=>path.includes('/deployment/')?{config}:{preferences:{theme:'dark'}});
    initializeOfficeAppearance();await vi.waitFor(()=>expect(root.dataset.theme).toBe('dark'));expect(style.has('--primary')).toBe(false);
  });
  it('preserves system appearance rather than silently forcing light mode',async()=>{
    vi.mocked(apiJson).mockImplementation(async path=>path.includes('/deployment/')?{config}:{preferences:{theme:'system'}});
    initializeOfficeAppearance();await vi.waitFor(()=>expect(root.dataset.theme).toBe('system'));expect(style.has('--primary')).toBe(false);
  });
  it('does not let invalid cache contents or denied storage prevent opening',()=>{
    storage.set('mx_office_appearance',JSON.stringify({theme:'dark',accent:'url(bad)',fontPercent:9999}));
    initializeOfficeAppearance();expect(root.dataset.theme).toBe('light');expect(root.style.fontSize).toBe('100%');
    vi.stubGlobal('sessionStorage',{getItem:()=>{throw Error('Denied');},setItem:()=>{throw Error('Denied');}});
    expect(()=>initializeOfficeAppearance()).not.toThrow();expect(()=>rememberOfficeAppearance()).not.toThrow();
  });
});
