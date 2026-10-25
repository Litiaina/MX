import {apiJson,jsonRequest} from './client';
export type ModuleViewKind='records'|'board'|'cards'|'overview'|'documents'|'ledger'|'inventory';
export interface ModuleView {id:string;name:string;kind:ModuleViewKind;title_field:string;fields:string[];summary_fields:string[];description:string}
export interface RecordSection {name:string;fields:string[]}
export interface ModuleLayout {default_view:string;views:ModuleView[];sections:RecordSection[]}
export interface ModuleWorkspaceConfiguration {revision:number;layout:ModuleLayout}
const base=(module:string)=>`/mx/v1/modules/${encodeURIComponent(module)}/workspace`;
export const loadModuleWorkspace=(module:string,signal?:AbortSignal)=>apiJson<ModuleWorkspaceConfiguration>(base(module),{signal});
export const saveModuleWorkspace=(module:string,revision:number,layout:ModuleLayout)=>apiJson<ModuleWorkspaceConfiguration>(base(module),jsonRequest('PUT',{revision,layout}));
