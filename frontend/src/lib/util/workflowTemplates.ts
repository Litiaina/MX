import type {ProcessKind,WorkflowDefinition,WorkflowAction} from '../api/workflows';
/** Drafts only: choosing one never creates modules, fields or records. */
export function workflowTemplate(kind:ProcessKind):WorkflowDefinition {
  const actions:WorkflowAction[]=[];
  const add=(key:string,name:string,from:string,to:string,review=false)=>actions.push({key,name,from,to,access_levels:review?[0,1]:[0,1,2],creator_policy:review?'not_creator':'any',required_fields:[],note_required:review});
  if(kind==='crm'){
    add('qualify','Qualify opportunity','lead','qualified');add('propose','Send proposal','qualified','proposal');add('win','Mark won','proposal','won');add('lose','Mark lost','proposal','lost');add('reopen','Reopen opportunity','lost','lead');
    return {enabled:false,name:'Opportunity pipeline',kind,initial_state:'lead',title_field:'',restart_on_edit:false,states:[{key:'lead',name:'Lead'},{key:'qualified',name:'Qualified'},{key:'proposal',name:'Proposal'},{key:'won',name:'Won'},{key:'lost',name:'Lost'}],actions};
  }
  add('submit','Submit for review','draft','review');add('approve','Approve request','review','approved',true);add('return','Return for changes','review','draft',true);add('cancel','Cancel request','draft','cancelled');add('reopen','Reopen request','cancelled','draft');
  return {enabled:false,name:kind==='erp'?'Purchase-request approval':kind==='accounting'?'Expense-request approval':'Custom process',kind,initial_state:'draft',title_field:'',restart_on_edit:true,states:[{key:'draft',name:'Draft'},{key:'review',name:'Awaiting review'},{key:'approved',name:'Approved'},{key:'cancelled',name:'Cancelled'}],actions};
}
export const workflowKindNames:Record<ProcessKind,string>={custom:'Custom process',crm:'CRM · opportunity pipeline',erp:'ERP · purchase-request approval',accounting:'Accounting · expense-request approval'};
