import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
const read=(path:string)=>readFileSync(new URL(path,import.meta.url),'utf8');

describe('Drive cascade regression',()=>{
  it('keeps scoped Drive styles out of lower-priority cascade layers',()=>{
    for(const path of ['../lib/components/DriveView.svelte','../lib/components/DriveFileIcon.svelte','../lib/components/DrivePagination.svelte']) {
      const style=read(path).match(/<style>([\s\S]*?)<\/style>/)?.[1];
      expect(style,path).toBeTruthy();expect(style,path).not.toContain('@layer');
    }
  });
  it('does not load the withdrawn global polish overrides',()=>{
    expect(read('../main.ts')).not.toContain('styles/polish.css');
    expect(read('./tokens.css')).toContain('@layer legacy, tokens, foundation, shell, workspaces;');
  });
  it('gives Deployment independent stacks without adding application-wide form selectors',()=>{
    const source=read('../lib/components/admin/OperationsAdmin.svelte');
    expect((source.match(/class="settings-column"/g)||[]).length).toBe(2);
    expect(source).toContain('.settings-column { min-width: 0; display: grid; align-content: start; gap: 1rem; }');
    expect(source).toContain('.settings-grid input');
    expect(source).not.toContain(':global(');
  });
});
