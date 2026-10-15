import {apiJson,apiUpload,jsonRequest} from './client';
import {operationUid} from './operation';
export interface UpdatePolicy {mode:'manual'|'automatic';start_hour:number;end_hour:number;utc_offset_minutes:number}
export interface UpdateOperation {uid:string;action:string;phase:string;message:string;version:string|null;bytes:number;total:number;started_at:number;updated_at:number;error:string|null}
export interface UpdateStatus {
  configured:boolean;current_version:string;message?:string;channel?:string;repository?:string;managed?:boolean;
  policy?:UpdatePolicy;notice_seconds?:number;busy?:boolean;maintenance?:boolean;last_checked_at?:number|null;worker_last_started_at?:number|null;
  available?:{version:string;tag:string;notes:string}|null;staged?:{version:string}|null;operation?:UpdateOperation|null;history?:UpdateOperation[];
}
export interface UpdateHealth {configured:boolean;version:string;ready:boolean;maintenance:boolean;scheduled_at:number|null}
export const loadUpdateStatus=(signal?:AbortSignal)=>apiJson<UpdateStatus>('/mx/v1/admin/updates',{signal,cache:'no-store',timeoutMs:5000,maxAttempts:1});
export const loadUpdateHealth=(signal?:AbortSignal)=>apiJson<UpdateHealth>('/mx/v1/system/update-health',{signal,cache:'no-store',timeoutMs:5000,maxAttempts:1},false);
export const queueUpdate=(action:'check'|'stage'|'install',version?:string,operation_uid=operationUid())=>apiJson<{operation_uid:string}>(`/mx/v1/admin/updates/${action}`,{...jsonRequest('POST',{operation_uid,version:version??null}),timeoutMs:10000,maxAttempts:1});
export const saveUpdatePolicy=(policy:UpdatePolicy)=>{
  // Switching to manual must not trap the operator behind invalid, now-hidden
  // draft automatic hours. Keep valid schedules; normalize only an equal pair.
  const submitted=policy.mode==='manual'&&policy.start_hour===policy.end_hour?{...policy,end_hour:(policy.start_hour+1)%24}:policy;
  return apiJson('/mx/v1/admin/updates/policy',{...jsonRequest('PUT',submitted),timeoutMs:10000,maxAttempts:1});
};
export const uploadUpdatePackage=(file:File,onProgress?:(loaded:number,total:number)=>void,signal?:AbortSignal,uid=operationUid())=>apiUpload<{operation_uid:string}>(`/mx/v1/admin/updates/package?operation_uid=${encodeURIComponent(uid)}`,file,onProgress,true,signal);
export const updateActive=(operation?:UpdateOperation|null)=>!!operation&&!['completed','failed','rolled_back','deferred'].includes(operation.phase);
export function offsetLabel(minutes:number) {const sign=minutes<0?'−':'+';const absolute=Math.abs(minutes);return `UTC${sign}${String(Math.floor(absolute/60)).padStart(2,'0')}:${String(absolute%60).padStart(2,'0')}`;}
