import {describe,it,expect} from 'vitest';
import {workflowTemplate} from './workflowTemplates';
describe('Opt-in workflow drafts',()=>{
  for(const kind of ['crm','erp','accounting','custom'] as const)it(`${kind} has valid connected actions and no automatic enablement`,()=>{
    const d=workflowTemplate(kind);expect(d.enabled).toBe(false);expect(d.states.some(s=>s.key===d.initial_state)).toBe(true);expect(d.title_field).toBe('');
    const keys=new Set(d.states.map(s=>s.key));expect(keys.size).toBe(d.states.length);
    for(const a of d.actions){expect(keys.has(a.from)&&keys.has(a.to)).toBe(true);expect(a.from).not.toBe(a.to);expect(a.required_fields).toEqual([]);}
    expect(d.restart_on_edit).toBe(kind!=='crm');
    if(kind==='erp'||kind==='accounting'){const approval=d.actions.find(a=>a.key==='approve')!;expect(approval.creator_policy).toBe('not_creator');expect(approval.access_levels).toEqual([0,1]);expect(approval.note_required).toBe(true);}
  });
  it('returns independent definitions',()=>{const d=workflowTemplate('erp');d.actions[0].access_levels.push(3);expect(workflowTemplate('erp').actions[0].access_levels).not.toContain(3);});
});
