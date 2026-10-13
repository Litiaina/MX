import type {JsonValue} from '../api/domain';
import type {RelationshipPreview} from '../api/relationships';

// A response arriving after the user typed into a destination must not erase it.
export function applyRelationshipPreview(values:Record<string,JsonValue>, before:Record<string,JsonValue>, preview:RelationshipPreview) {
  const next={...values,...preview.computed};
  for(const [key,value] of Object.entries(preview.values)) {
    if(JSON.stringify(values[key]??null)===JSON.stringify(before[key]??null)) next[key]=value;
  }
  return next;
}
