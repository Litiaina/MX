import { describe, expect, it } from 'vitest';
import type { ModuleDefinition } from '../api/domain';
import { matchingModules, moduleGroup, orderedModules } from './moduleNavigation';

const module = (uid: string, name: string, position: number, group = '', active = true): ModuleDefinition => ({
  uid, name, position, active, slug: uid, singular_name: name, description: 'Employee information', icon: 'database', color: '#1d4ed8', config: {navigation_group:group}, permissions:[]
});

describe('Module navigation', () => {
  it('uses configured order with deterministic name/UID ties without mutating the supplied list', () => {
    const modules = [module('z','Zulu',20),module('c','Beta',10),module('b','Alpha',10),module('a','Alpha',10)];
    expect(orderedModules(modules).map(item=>item.uid)).toEqual(['a','b','c','z']);
    expect(modules.map(item=>item.uid)).toEqual(['z','c','b','a']);
  });
  it('searches name, description and group together, case-insensitively', () => {
    const hr = module('hr','Attendance',10,' Human Resources '), inventory = module('inventory','Stock',20,'Supplies');
    expect(moduleGroup(hr)).toBe('Human Resources');
    expect(matchingModules([hr,inventory],'HUMAN employee')).toEqual([hr]);
    expect(matchingModules([hr,inventory],'attendance','Supplies')).toEqual([]);
    expect(matchingModules([hr,inventory],'','Supplies')).toEqual([inventory]);
  });
  it('excludes archived modules and safely handles missing/malformed optional group data', () => {
    const current=module('current','Current',20), archived=module('archived','Archived',0,'',false);
    current.config.navigation_group=123;
    expect(moduleGroup(current)).toBe('');
    expect(orderedModules([archived,current])).toEqual([current]);
  });
  it('finds one workspace among 100 modules while respecting administrator order', () => {
    const modules = Array.from({length:100},(_,index)=>module(String(index),`Department ${String(index).padStart(3,'0')}`,100-index));
    expect(orderedModules(modules)[0].uid).toBe('99');
    expect(matchingModules(modules,'department 042').map(item=>item.uid)).toEqual(['42']);
    expect(matchingModules(modules,'nothing')).toEqual([]);
  });
});
