import {afterEach,describe,expect,it,vi} from 'vitest';
import {applyBrandIcon,brandIconUrl} from './branding';
afterEach(()=>vi.unstubAllGlobals());
describe('Configured browser-tab branding',()=>{
  it('resolves paths against MX rather than the Office subdirectory',()=>{
    for(const path of ['images/custom.png','./images/custom.png','/images/custom.png'])expect(brandIconUrl(path,'https://mx.example')).toBe('https://mx.example/images/custom.png');
    expect(brandIconUrl('https://assets.example/logo.png','https://mx.example')).toBe('https://assets.example/logo.png');
  });
  it('rejects executable/data URLs, embedded credentials and malformed inputs',()=>{
    for(const value of ['',null,{},'javascript:alert(1)','data:image/svg+xml,bad','file:///logo','https://secret:password@example/logo'])expect(brandIconUrl(value,'https://mx.example')).toBe('https://mx.example/images/system-icon.png');
  });
  it('creates a missing icon, clears stale MIME types and ignores an old failed image',()=>{
    vi.stubGlobal('location',{origin:'https://mx.example'});
    let link:{rel?:string;href?:string;removeAttribute:ReturnType<typeof vi.fn>}|undefined;
    vi.stubGlobal('document',{querySelector:()=>link,createElement:()=>({removeAttribute:vi.fn()}),head:{append:(value:typeof link)=>{link=value;}}});
    const probes:{onerror?:()=>void;src?:string}[]=[];
    vi.stubGlobal('Image',class{constructor(){probes.push(this);}});
    applyBrandIcon('images/first.png');expect(link?.href).toBe('https://mx.example/images/first.png');expect(link?.rel).toBe('icon');
    applyBrandIcon('images/new.png');probes[0].onerror?.();expect(link?.href).toBe('https://mx.example/images/new.png');
    probes[1].onerror?.();expect(link?.href).toBe('https://mx.example/images/system-icon.png');
    expect(link?.removeAttribute).toHaveBeenCalledWith('type');
  });
});
