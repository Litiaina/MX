import { describe, expect, it } from 'vitest';
import { officePermissionQuery } from './compatibility';
describe('Office clipboard permission compatibility',()=>{
  it('keeps native permission results including denied',async()=>{
    const denied={state:'denied'} as PermissionStatus;
    const query=officePermissionQuery(async()=>denied);
    expect(await query({name:'clipboard-read' as PermissionName})).toBe(denied);
  });
  it('does not grant an unsupported clipboard permission',async()=>{
    const query=officePermissionQuery(async()=>{throw new TypeError('Unsupported name');});
    const status=await query({name:'clipboard-write' as PermissionName});
    expect(status.state).toBe('prompt');expect(status).toBeInstanceOf(EventTarget);
  });
  it('does not swallow other permission errors',async()=>{
    const query=officePermissionQuery(async()=>{throw new TypeError('Unsupported name');});
    await expect(query({name:'camera'})).rejects.toThrow('Unsupported name');
    const denied=officePermissionQuery(async()=>{throw new Error('Not allowed');});
    await expect(denied({name:'clipboard-read' as PermissionName})).rejects.toThrow('Not allowed');
  });
});
