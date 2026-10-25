import type { ModuleDefinition } from '../api/domain';

export function moduleGroup(module: ModuleDefinition): string {
  return typeof module.config.navigation_group === 'string' ? module.config.navigation_group.trim() : '';
}

export function orderedModules(modules: ModuleDefinition[]): ModuleDefinition[] {
  return modules.filter(module => module.active).slice().sort((a, b) =>
    a.position - b.position || a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }) || a.uid.localeCompare(b.uid));
}

export function matchingModules(modules: ModuleDefinition[], query: string, group = ''): ModuleDefinition[] {
  const words = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return orderedModules(modules).filter(module => {
    const category = moduleGroup(module);
    const text = `${module.name} ${module.description} ${category}`.toLocaleLowerCase();
    return (!group || category === group) && words.every(word => text.includes(word));
  });
}
