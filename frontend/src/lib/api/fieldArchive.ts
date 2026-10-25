import {ApiError,apiJson} from './client';
import type {FieldDefinition} from './domain';
export interface ArchiveBlocker {
  kind:string;module_uid:string;module_name:string;field_uid:string|null;field_label:string|null;
  field_key:string|null;field_type:string|null;reason:string;resolution:string;section:'field'|'storage'|'records'|'workflow'|'views';count:number|null;
}
export interface FieldArchiveCheck {
  field:FieldDefinition;module_uid:string;module_name:string;can_archive:boolean;blockers:ArchiveBlocker[];
}
export const checkFieldArchive=(uid:string,signal?:AbortSignal)=>apiJson<FieldArchiveCheck>(`/mx/v1/admin/schema/fields/${encodeURIComponent(uid)}/archive-check`,{signal});
export function archiveFailure(reason:unknown,uid:string):FieldArchiveCheck|null {
  if(!(reason instanceof ApiError)||!reason.payload||typeof reason.payload!=='object')return null;
  const body=reason.payload as {error?:unknown;archive_check?:FieldArchiveCheck};
  if(body.error!=='field_archive_blocked'||body.archive_check?.field?.uid!==uid||!Array.isArray(body.archive_check.blockers))return null;
  return body.archive_check;
}
