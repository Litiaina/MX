import {apiJson} from './client';
export interface LinkedRecord {uid:string;label:string}
export interface RelationshipPage {items:LinkedRecord[];page:number;has_next:boolean}
export interface RelatedRecord {uid:string;module_uid:string;module_name:string;label:string;relationship_label:string}
export const relationshipOptions=(field:string,q='',page=1,signal?:AbortSignal)=>apiJson<RelationshipPage>(`/mx/v1/relationships/${encodeURIComponent(field)}/options?${new URLSearchParams({q,page:String(page)})}`,{signal});
export const relatedRecords=(module:string,record:string,page=1,signal?:AbortSignal)=>apiJson<{items:RelatedRecord[];page:number;has_next:boolean}>(`/mx/v1/relationships/${encodeURIComponent(module)}/${encodeURIComponent(record)}/related?page=${page}`,{signal});
