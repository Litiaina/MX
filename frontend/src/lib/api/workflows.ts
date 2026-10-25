import {apiJson,jsonRequest} from './client';
export type ProcessKind='custom'|'crm'|'erp'|'accounting';
export interface WorkflowState {key:string;name:string}
export interface WorkflowAction {key:string;name:string;from:string;to:string;access_levels:number[];creator_policy:'any'|'creator'|'not_creator';required_fields:string[];note_required:boolean}
export interface WorkflowDefinition {enabled:boolean;name:string;kind:ProcessKind;initial_state:string;title_field:string;restart_on_edit:boolean;states:WorkflowState[];actions:WorkflowAction[]}
export interface WorkflowConfiguration {revision:number;definition:WorkflowDefinition;existing_records?:number|null}
export interface AvailableAction {key:string;name:string;to:string;to_name:string;note_required:boolean;missing_fields:string[];blocked_reason?:string|null}
export interface WorkflowStatus {enabled:boolean;has_history?:boolean;name?:string;kind?:ProcessKind;state?:string;state_name?:string;workflow_revision?:number;configuration_revision:number;record_revision?:number;changed_since_action?:boolean;restarted?:boolean;actions?:AvailableAction[]}
export interface WorkQueue {enabled:boolean;name?:string;states?:WorkflowState[];configuration_revision?:number;items:(WorkflowStatus & {uid:string;title:string})[];total?:number|null;stage_totals?:{state:string;total:number}[];has_next:boolean;next_cursor:string|null}
export interface WorkflowEvent {uid:string;sequence:number;action_name:string;from_name:string;to_name:string;actor_uid:string;actor_name:string;note:string;record_revision:number;snapshot_uid:string;created_at:number;configuration_revision:number}
export interface WorkflowHistory {items:WorkflowEvent[];has_next:boolean;next_cursor:number|null}
export interface WorkflowOperation {operation_uid:string;action:string;configuration_revision:number;workflow_revision:number;record_revision:number;note:string}
const base=(module:string)=>`/mx/v1/modules/${encodeURIComponent(module)}`;
const recordBase=(module:string,record:string)=>`${base(module)}/records/${encodeURIComponent(record)}/workflow`;
export const loadWorkflow=(module:string,signal?:AbortSignal)=>apiJson<WorkflowConfiguration>(`${base(module)}/workflow`,{signal});
export const saveWorkflow=(module:string,revision:number,definition:WorkflowDefinition,confirm_existing_records:boolean)=>apiJson<WorkflowConfiguration>(`${base(module)}/workflow`,jsonRequest('PUT',{revision,definition,confirm_existing_records}));
export const loadWorkflowStatus=(module:string,record:string,signal?:AbortSignal)=>apiJson<WorkflowStatus>(recordBase(module,record),{signal});
export const workflowAction=(module:string,record:string,operation:WorkflowOperation)=>apiJson<{event_uid:string;replayed:boolean;status:WorkflowStatus}>(`${recordBase(module,record)}/actions`,{...jsonRequest('POST',operation),retrySafe:true,maxAttempts:2});
export const loadWorkQueue=(module:string,query:{after?:string;state?:string;actionable?:boolean;q?:string;counts_only?:boolean;include_total?:boolean}={},signal?:AbortSignal)=>{
  const params=new URLSearchParams();if(query.after)params.set('after',query.after);if(query.state)params.set('state',query.state);if(query.actionable)params.set('actionable','true');
  if(query.q)params.set('q',query.q);
  if(query.counts_only)params.set('counts_only','true');
  if(query.include_total)params.set('include_total','true');
  return apiJson<WorkQueue>(`${base(module)}/work-queue?${params}`,{signal});
};
export const loadWorkflowHistory=(module:string,record:string,before?:number,signal?:AbortSignal)=>apiJson<WorkflowHistory>(`${recordBase(module,record)}/history${before?`?before=${before}`:''}`,{signal});
