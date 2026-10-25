import {apiJson,jsonRequest} from './client';
export interface BookSettings {accounting:boolean;inventory:boolean;currency:string;money_decimals:number;stock_decimals:number;operator_levels:number[];closed_through:string;required_stage:string}
export interface BookConfiguration {revision:number;settings:BookSettings;can_configure:boolean;can_operate:boolean}
export interface BookSource {record_uid:string;record_revision:number}
export interface BookLine {account_uid:string;debit:string;credit:string}
export type BookAction = {kind:'account';code:string;name:string;category:string}|{kind:'item';code:string;name:string;unit:string}|{kind:'journal';date:string;description:string;source:BookSource|null;lines:BookLine[]}|{kind:'reverse';journal_uid:string;date:string;reason:string}|{kind:'stock';item_uid:string;movement:string;quantity:string;note:string;source:BookSource|null}|{kind:'reservation';reservation_uid:string;action:string;note:string};
export type BookCommand=BookAction&{operation_uid:string;configuration_revision:number};
export interface BookRow {sequence:number;uid:string;code?:string;name?:string;category?:string;debit?:string;credit?:string;balance?:string;unit?:string;on_hand?:string;reserved?:string;available?:string;quantity?:string;status?:string;note?:string;date?:string;description?:string;movement?:string;reversal_of?:string|null;reversed_by?:string|null;actor_uid?:string;created_at?:number}
export interface BookPage {items:BookRow[];has_next:boolean;next_cursor:number|null}
export interface JournalDetail {uid:string;date:string;description:string;actor_uid:string;actor_name:string|null;source_uid:string|null;snapshot_uid:string|null;reversal_of:string|null;created_at:number;lines:{account_uid:string;code:string;name:string;debit:string;credit:string}[]}
export type BookList='accounts'|'items'|'journals'|'reservations'|'stock-events';
const base=(module:string)=>`/mx/v1/modules/${encodeURIComponent(module)}/books`;
export const loadBooks=(module:string,signal?:AbortSignal)=>apiJson<BookConfiguration>(base(module),{signal});
export const saveBooks=(module:string,revision:number,settings:BookSettings)=>apiJson<BookConfiguration>(base(module),jsonRequest('PUT',{revision,settings}));
export const bookCommand=(module:string,command:BookCommand)=>apiJson<{uid:string;replayed:boolean}>(`${base(module)}/commands`,{...jsonRequest('POST',command),retrySafe:true,maxAttempts:2});
export const listBook=(module:string,kind:BookList,after=0,signal?:AbortSignal)=>apiJson<BookPage>(`${base(module)}/list/${kind}?after=${after}`,{signal});
export const journalDetail=(module:string,uid:string,signal?:AbortSignal)=>apiJson<JournalDetail>(`${base(module)}/journals/${encodeURIComponent(uid)}`,{signal});
