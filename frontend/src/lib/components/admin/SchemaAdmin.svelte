<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import GripVertical from '@lucide/svelte/icons/grip-vertical';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import Plus from '@lucide/svelte/icons/plus';
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import Search from '@lucide/svelte/icons/search';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import ModuleIcon from '../ModuleIcon.svelte';
  import { focusDialog } from '../../util/focusDialog';
  import { dismissBackdrop } from '../../util/dismissBackdrop';
  import type { FieldDefinition, JsonValue, ModuleDefinition, ModulePermission, SchemaResponse } from '../../api/domain';
  import type { Session } from '../../api/types';
  import { createField, createModule, deleteModule, loadModules, loadModuleSchema, loadSchema, loadStorageLayout, saveStorageLayout, updateField, updateModule, updateSchemaOrder, updateSystemField } from '../../api/workspace';
  import { cloneJson } from '../../util/json';
  import RelationshipConfig from './RelationshipConfig.svelte';
  import ColumnSummariesAdmin from './ColumnSummariesAdmin.svelte';
  import FormRulesAdmin from './FormRulesAdmin.svelte';
  import WorkflowAdmin from './WorkflowAdmin.svelte';
  import BooksSettings from './BooksSettings.svelte';
  import ModuleViewsAdmin from './ModuleViewsAdmin.svelte';
  import FormulaConfig from './FormulaConfig.svelte';
  import FieldRulesDialog from './FieldRulesDialog.svelte';
  import FieldArchiveDialog from './FieldArchiveDialog.svelte';
  import type {ArchiveBlocker} from '../../api/fieldArchive';
  import { fieldKeyFailure, generatedFieldKey, type FieldKeyConflict } from '../../api/fieldKeys';
  import SchemaErrorDialog from './SchemaErrorDialog.svelte';
  import { requestConfirmation } from '../../confirmation';
  import { MODULE_ICON_OPTIONS, normaliseModuleIcon } from '../../util/moduleIcons';

  let { session }: { session: Session } = $props();

  let schema = $state<SchemaResponse | null>(null); let loading = $state(true); let error = $state(''); let notice = $state('');
  let errorPopup = $state<{title:string;message:string;conflict:FieldKeyConflict|null;target:string}|null>(null);
  let createError = $state('');
  let createKeyInvalid = $state(false);
  let archiveTarget=$state<{uid:string;label:string}|null>(null);
  let rulesTarget=$state<FieldDefinition|null>(null);
  let fieldFocusUid=$state('');let savedFieldSchema=$state('');
  let savedFieldTypes=$state<Record<string,FieldDefinition['field_type']>>({});
  let label = $state(''); let key = $state(''); let type = $state<FieldDefinition['field_type']>('text'); let required = $state(false); let unique = $state(false); let searchable = $state(true); let sortable = $state(true); let visible = $state(true); let priority = $state(50); let options = $state('');
  let autoScope = $state('global'); let autoPrefix = $state(''); let autoPadding = $state(0);
  let relationshipConfig = $state<Record<string,JsonValue>>({});
  let formulaExpression = $state(''); let formulaDecimals = $state(2);
  let formulaConfig=$state<Record<string,JsonValue>>({expression:'',decimals:2});
  let attachmentStorage = $state(''); let attachmentDescription = $state(''); let attachmentMultiple = $state(true); let attachmentMaxFiles = $state<number | undefined>(undefined);
  let folders = $state<string[]>([]); let storageCandidate = $state(''); let prefix = $state(''); let preview = $state(''); let storageRoot = $state('records'); let frozen = $state(0); let storageRevision = $state(0); let storageConfigured = $state(false);
  let savedFolders = $state<string[]>([]); let savedPrefix = $state('');
  let modules = $state<ModuleDefinition[]>([]); let selectedModuleUid = $state('');
  let moduleSearch = $state('');
  type Section = 'fields' | 'details' | 'access' | 'rules' | 'summaries' | 'storage' | 'workflow' | 'books' | 'views';
  let viewsDirty = $state(false);
  let workflowDirty = $state(false);
  let booksDirty = $state(false);
  let section = $state<Section>('fields');
  const sections: {id:Section;name:string;description:string}[] = [
    {id:'fields',name:'Fields',description:'Add and configure the information your records store.'},
    {id:'details',name:'Module details',description:'Name, navigation, description and appearance for this module.'},
    {id:'views',name:'Views & layout',description:'Design this module’s main screen: pipeline, operations, directory, documents, accounting, stock or records.'},
    {id:'access',name:'Permissions',description:'Choose what each account level may do in this module.'},
    {id:'rules',name:'Form rules',description:'Show, require or fill fields based on a person’s choices.'},
    {id:'workflow',name:'Process & actions',description:'Configure CRM pipelines, ERP or expense approvals, or your own staged process.'},
    {id:'books',name:'Accounting & stock',description:'Opt in to exact journals, stock balances, reservations and posting controls.'},
    {id:'summaries',name:'Column totals',description:'Summarize numeric values across matching records.'},
    {id:'storage',name:'File storage',description:'Configure this module’s independent N1 attachment layout.'}
  ];
  const matchingModules = $derived(modules.filter(module=>`${module.name} ${module.description}`.toLocaleLowerCase().includes(moduleSearch.trim().toLocaleLowerCase())));
  let refreshGeneration=0; let destroyed=false;
  onDestroy(()=>{destroyed=true;refreshGeneration++;});
  let moduleName = $state(''); let moduleSingular = $state(''); let moduleDescription = $state(''); let moduleIcon = $state('file-text'); let moduleColor = $state('#1d4ed8');
  let modulePosition = $state(0); let moduleActive = $state(true); let moduleNavigationGroup = $state('');
  let moduleRecordVisibility=$state('module');
  let modulePermissions = $state<ModulePermission[]>([]);
  let moduleCreateOpen = $state(false); let moduleCreateBusy = $state(false); let moduleCreateError = $state('');
  let newModuleName = $state(''); let newModuleSingular = $state(''); let newModuleDescription = $state(''); let newModuleIcon = $state('file-text'); let newModuleColor = $state('#3768a6'); let newModuleNavigationGroup = $state('');
  let deletingModule = $state<ModuleDefinition | null>(null); let deleteConfirmation = $state(''); let deletePassword = $state(''); let deleteFactor = $state(''); let deleteError = $state(''); let deleteBusy = $state(false);
  const activeFields = $derived(schema?.fields.filter((field) => field.active).sort((a, b) => a.position - b.position) || []);
  const storageFields = $derived(activeFields.filter((field) => !['attachments', 'relationship', 'lookup', 'rollup'].includes(field.field_type)));
  const selectedModule = $derived(modules.find((module) => module.uid === selectedModuleUid) || null);
  const proposedKey = $derived(key.trim().toLowerCase() || generatedFieldKey(label));
  const keyOwner = $derived(schema?.fields.find(field => field.key.toLowerCase() === proposedKey));
  const moduleDirty = $derived.by(() => {
    if (!selectedModule) return false;
    const savedNavigationGroup = typeof selectedModule.config.navigation_group === 'string' ? selectedModule.config.navigation_group : '';
    return moduleName !== selectedModule.name
      || moduleSingular !== selectedModule.singular_name
      || moduleDescription !== selectedModule.description
      || moduleIcon !== normaliseModuleIcon(selectedModule.icon)
      || moduleColor.toLocaleLowerCase() !== selectedModule.color.toLocaleLowerCase()
      || Number(modulePosition) !== Number(selectedModule.position)
      || moduleActive !== selectedModule.active
      || moduleRecordVisibility !== String(selectedModule.config.record_visibility||'module')
      || moduleNavigationGroup !== savedNavigationGroup
      || JSON.stringify(modulePermissions) !== JSON.stringify(selectedModule.permissions);
  });
  const storageDirty = $derived(JSON.stringify(folders) !== JSON.stringify(savedFolders) || prefix !== savedPrefix);
  const fieldsDirty=$derived(!!schema&&JSON.stringify(schema.fields)!==savedFieldSchema);
  const newFieldDirty = $derived(!!(label || key || options || type !== 'text' || formulaExpression || attachmentStorage || attachmentDescription || required || unique || !searchable || !sortable || !visible || priority !== 50));
  const moduleWorkspaceDirty = $derived(moduleDirty || storageDirty || fieldsDirty || newFieldDirty || workflowDirty || booksDirty || viewsDirty);
  async function switchSection(next:Section){if(next===section)return;if((workflowDirty||booksDirty||viewsDirty)&&!await requestConfirmation({title:'Discard unsaved module configuration?',description:'Save your workspace, process or book configuration before leaving, or discard its unsaved changes.',confirmLabel:'Discard and switch',tone:'danger'}))return;workflowDirty=false;booksDirty=false;viewsDirty=false;section=next;}

  onMount(() => void initialise());
  async function initialise() { try { const response=await loadModules(); if(destroyed)return; modules=response.modules;const params=new URLSearchParams(location.hash.split('?')[1]||'');const uid=params.get('module');if(uid&&modules.some(m=>m.uid===uid)){selectedModuleUid=uid;syncModuleEditor();section=params.get('section')==='views'?'views':'fields';await refresh();} } catch (reason) { if(!destroyed)fail(reason,'Could not load modules'); } finally { if(!destroyed)loading=false; } }
  function resetFieldDraft(){label='';key='';type='text';options='';required=false;unique=false;searchable=true;sortable=true;visible=true;priority=50;autoScope='global';autoPrefix='';autoPadding=0;relationshipConfig={};formulaExpression='';formulaDecimals=2;formulaConfig={expression:'',decimals:2};attachmentStorage='';attachmentDescription='';attachmentMultiple=true;attachmentMaxFiles=undefined;createError='';createKeyInvalid=false;}
  function syncModuleEditor() { const module = modules.find((item) => item.uid === selectedModuleUid); if (!module) return; moduleName = module.name; moduleSingular = module.singular_name; moduleDescription = module.description; moduleIcon = normaliseModuleIcon(module.icon); moduleColor = module.color; modulePosition = module.position; moduleActive = module.active; moduleNavigationGroup = typeof module.config.navigation_group === 'string' ? module.config.navigation_group : ''; modulePermissions = cloneJson(module.permissions);moduleRecordVisibility=String(module.config.record_visibility||'module'); }
  async function switchModule(nextUid: string, control?: HTMLSelectElement) {
    if (loading || nextUid === selectedModuleUid) return;
    if (moduleWorkspaceDirty && !await requestConfirmation({
      title: `Discard unsaved changes to ${selectedModule?.name || 'this module'}?`,
      description: `You are switching to ${modules.find((module) => module.uid === nextUid)?.name || 'another module'}. Unsaved module, field or N1 storage changes will be lost.`,
      confirmLabel: 'Discard and switch',
      tone: 'danger'
    })) {
      if (control) control.value = selectedModuleUid;
      return;
    }
    selectedModuleUid = nextUid;
    schema=null;
    resetFieldDraft(); section='fields';
    workflowDirty=false;booksDirty=false;viewsDirty=false;
    syncModuleEditor();
    notice = '';
    await refresh();
  }
  async function refresh() {
    const uid=selectedModuleUid, generation=++refreshGeneration;
    if(!uid){schema=null;loading=false;return;}
    loading=true;error='';
    try {
      const [next,layout]=await Promise.all([uid==='mx-default-records'?loadSchema(true):loadModuleSchema(uid),loadStorageLayout(uid)]);
      if(destroyed||generation!==refreshGeneration||uid!==selectedModuleUid)return;
      schema=next;savedFieldSchema=JSON.stringify(next.fields);savedFieldTypes=Object.fromEntries(next.fields.map(field=>[field.uid,field.field_type]));
      folders=layout.folder_fields.map(field=>field.uid);prefix=layout.file_prefix_field?.uid||'';savedFolders=[...folders];savedPrefix=prefix;preview=layout.preview;storageRoot=layout.root;frozen=layout.frozen_records;storageRevision=layout.revision;storageConfigured=layout.configured;
    } catch(reason){if(!destroyed&&generation===refreshGeneration)fail(reason,'Could not load module settings');}
    finally{if(!destroyed&&generation===refreshGeneration)loading=false;}
  }
  function fail(reason: unknown, title = 'Could not save these settings', target = '') {
    error = reason instanceof Error ? reason.message : 'The operation failed.'; notice = '';
    // Archive/rules dialogs own their errors. Never put a second modal over them.
    if (!archiveTarget && !rulesTarget) errorPopup = { title, message:error, conflict:fieldKeyFailure(reason), target };
  }
  async function closeError() {
    const target = errorPopup?.target; errorPopup = null; await tick();
    if (target) { const control = document.querySelector<HTMLElement>(target); control?.scrollIntoView({block:'center'}); control?.focus({preventScroll:true}); }
  }
  async function useAvailableKey(value:string) { key=value; createError=''; createKeyInvalid=false; error=''; await closeError(); }
  async function viewKeyOwner(conflict:FieldKeyConflict) {
    errorPopup=null; await tick(); await switchModule(conflict.field.module_uid);
    if (selectedModuleUid !== conflict.field.module_uid) return;
    section='fields';
    fieldFocusUid=conflict.field.uid; await tick();
    const card=document.querySelector<HTMLElement>(`[data-schema-field="${CSS.escape(conflict.field.uid)}"]`);
    card?.scrollIntoView({block:'center'}); card?.focus({preventScroll:true});
  }
  function configFor(fieldType: string, currentOptions = ''): Record<string, JsonValue> { if (['relationship','lookup','rollup'].includes(fieldType)) return relationshipConfig; if (fieldType === 'select') return { options: currentOptions.split(',').map((v) => v.trim()).filter(Boolean) }; if (fieldType === 'attachments') { const maximum = Number(attachmentMaxFiles); return { storage_name: attachmentStorage || key || label, description: attachmentDescription, multiple: attachmentMultiple, max_files: attachmentMultiple && Number.isFinite(maximum) && maximum > 0 ? maximum : attachmentMultiple ? null : 1 }; } if (fieldType === 'auto_number') return { scope: autoScope, prefix: autoPrefix, padding: autoPadding }; if (fieldType === 'formula') return formulaConfig; return {}; }
  function syncType() { relationshipConfig = {}; if (['relationship', 'lookup', 'rollup'].includes(type)) { unique = false; searchable = false; sortable = false; if (type !== 'relationship') required = false; return; } if (type === 'auto_number') { required = true; unique = true; sortable = true; } else if (type === 'formula') { required = false; unique = false; searchable = true; sortable = true; } else if (type === 'attachments') { unique = false; searchable = true; sortable = false; } else if (type === 'long_text') sortable = false; else sortable = true; }
  async function add(event: SubmitEvent) { event.preventDefault(); if (loading) return; createError = ''; createKeyInvalid = false; error = ''; loading = true; try { await createField({ label, key: proposedKey, field_type: type, required: type === 'auto_number' ? true : ['formula','lookup','rollup'].includes(type) ? false : required, unique_value: type === 'auto_number' ? true : ['attachments', 'formula', 'relationship', 'lookup', 'rollup'].includes(type) ? false : unique, searchable, sortable: ['long_text', 'attachments', 'relationship', 'lookup', 'rollup'].includes(type) ? false : sortable, table_visible: visible, table_priority: priority, config: configFor(type, options) }, selectedModuleUid); label = ''; key = ''; type = 'text'; options = ''; autoScope = 'global'; autoPrefix = ''; autoPadding = 0; formulaExpression = ''; formulaDecimals = 2; formulaConfig={expression:'',decimals:2}; attachmentStorage = ''; attachmentDescription = ''; attachmentMultiple = true; attachmentMaxFiles = undefined; required = false; unique = false; searchable = true; sortable = true; notice = 'Field created.'; await refresh(); } catch (reason) { createKeyInvalid = !!fieldKeyFailure(reason); createError = reason instanceof Error ? reason.message : 'The field could not be created.'; fail(reason, 'Could not create field', createKeyInvalid ? '#new-field-key' : '#new-field-form'); } finally { loading = false; } }
  function resetNewModule() { newModuleName = ''; newModuleSingular = ''; newModuleDescription = ''; newModuleIcon = 'file-text'; newModuleColor = '#3768a6'; newModuleNavigationGroup = ''; moduleCreateError = ''; }
  async function openModuleCreate() {
    if(loading)return;
    if (moduleWorkspaceDirty && !await requestConfirmation({ title: `Discard unsaved changes to ${selectedModule?.name || 'this module'}?`, description: 'The new-module workflow is separate from the current editor. Save your module and N1 layout changes first, or discard them before continuing.', confirmLabel: 'Discard and create new', tone: 'danger' })) return;
    if (moduleDirty) syncModuleEditor();
    if (storageDirty) { folders = [...savedFolders]; prefix = savedPrefix; }
    if(fieldsDirty&&schema){schema={...schema,fields:JSON.parse(savedFieldSchema)};}
    if(workflowDirty||booksDirty){workflowDirty=false;booksDirty=false;section='fields';}
    resetFieldDraft();
    resetNewModule();
    moduleCreateOpen = true;
  }
  let confirmingModuleClose = false;
  async function closeModuleCreate() {
    if (moduleCreateBusy || confirmingModuleClose) return;
    if (newModuleName || newModuleSingular || newModuleDescription || newModuleNavigationGroup || newModuleIcon !== 'file-text' || newModuleColor !== '#3768a6') {
      confirmingModuleClose = true;
      const discard = await requestConfirmation({ title: 'Discard this new module?', description: 'Your unsaved module details will be lost. No existing module will be changed.', confirmLabel: 'Discard draft', tone: 'danger' });
      confirmingModuleClose = false;
      if (!discard) return;
    }
    moduleCreateOpen = false; resetNewModule();
  }
  async function addModule(event: SubmitEvent) {
    event.preventDefault(); moduleCreateBusy = true; moduleCreateError = '';
    try {
      const result = await createModule({ name: newModuleName, singular_name: newModuleSingular || undefined, description: newModuleDescription, icon: newModuleIcon, color: newModuleColor, config: { navigation_group: newModuleNavigationGroup.trim() } });
      modules = [...modules, result.module].sort((a,b) => a.position-b.position); selectedModuleUid = result.module.uid; section='fields'; moduleCreateOpen = false; resetNewModule(); syncModuleEditor(); notice = `${result.module.name} created. You are now editing the new module; add its fields below.`; await refresh();
    } catch (reason) { moduleCreateError = reason instanceof Error ? reason.message : 'The module could not be created.'; }
    finally { moduleCreateBusy = false; }
  }
  async function saveModule(event?: SubmitEvent) { event?.preventDefault(); if (!selectedModule || !moduleDirty) return; loading = true; try { const result = await updateModule(selectedModule.uid, { name: moduleName, singular_name: moduleSingular, description: moduleDescription, icon: moduleIcon, color: moduleColor, position: modulePosition, active: moduleActive, config: { ...selectedModule.config, navigation_group: moduleNavigationGroup.trim(),record_visibility:moduleRecordVisibility }, permissions: modulePermissions }); modules = modules.map((module) => module.uid === result.module.uid ? result.module : module).sort((a,b) => a.position-b.position); syncModuleEditor(); notice = 'Module identity, navigation, and access matrix saved.'; } catch (reason) { fail(reason); } finally { loading = false; } }
  function discardModuleChanges() { syncModuleEditor(); notice = ''; error = ''; }
  function beginModuleDelete() { if (!selectedModule || selectedModule.uid === 'mx-default-records') return; deletingModule = selectedModule; deleteConfirmation = ''; deletePassword = ''; deleteFactor = ''; deleteError = ''; }
  function closeModuleDelete() { if (deleteBusy) return; deletingModule = null; deleteConfirmation = ''; deletePassword = ''; deleteFactor = ''; deleteError = ''; }
  function deletionSecondFactor() { const value = deleteFactor.trim(); return /^\d{6}$/.test(value) ? { admin_otp: value, admin_recovery_code: null } : { admin_otp: null, admin_recovery_code: value || null }; }
  async function confirmModuleDelete(event: SubmitEvent) {
    event.preventDefault();
    if (!deletingModule) return;
    if (deleteConfirmation.trim() !== deletingModule.name) { deleteError = `Type ${deletingModule.name} exactly to continue.`; return; }
    deleteBusy = true; deleteError = '';
    try {
      const removed = deletingModule;
      const result = await deleteModule(removed.uid, { confirmation: deleteConfirmation, admin_password: deletePassword, ...deletionSecondFactor() });
      modules = modules.filter((module) => module.uid !== removed.uid);
      selectedModuleUid = '';
      deletingModule = null; syncModuleEditor();
      notice = `${result.module_name} deleted. ${result.records_deleted} record${result.records_deleted === 1 ? '' : 's'} removed${result.attachments_moved_to_n1_trash ? `; ${result.attachments_moved_to_n1_trash} attachment${result.attachments_moved_to_n1_trash === 1 ? '' : 's'} moved to N1 trash` : ''}.`;
      error = ''; await refresh();
    } catch (reason) { deleteError = reason instanceof Error ? reason.message : 'The module could not be deleted.'; }
    finally { deleteBusy = false; }
  }
  async function save(field: FieldDefinition) { const baseType=savedFieldTypes[field.uid]; if(field.field_type!==baseType && !await requestConfirmation({title:`Change ${field.label} to Long text?`,description:'Existing values, record IDs, revisions and history stay intact. The field key and relationships stay the same. Long text is not sortable.',confirmLabel:'Change datatype',tone:'primary'}))return; try { const saved = await updateField(field.uid, { field_type:field.field_type, base_field_type:baseType, label: field.label, required: field.field_type === 'auto_number' ? true : field.required, unique_value: field.field_type === 'auto_number' ? true : ['attachments','relationship','lookup','rollup'].includes(field.field_type) ? false : field.unique_value, searchable: field.searchable, sortable: ['long_text', 'attachments', 'relationship', 'lookup', 'rollup'].includes(field.field_type) ? false : field.sortable, table_visible: field.table_visible, table_priority: field.table_priority, config: field.config }); if(saved.field_type!==field.field_type)throw new Error('The running MX server did not apply this datatype change. Rebuild/restart the updated server before retrying; your existing field values are retained.'); notice = `${field.label} saved.`; await refresh(); } catch (reason) { fail(reason, `Could not save ${field.label}`, `[data-schema-field="${CSS.escape(field.uid)}"]`); } }
  async function toggle(field: FieldDefinition) { if(field.active){error='';notice='';archiveTarget={uid:field.uid,label:field.label};return;} if (!await requestConfirmation({ title: `Restore ${field.label}?`, description: 'Historical values will be preserved.', confirmLabel: 'Restore field', tone: 'primary' })) return; try { await updateField(field.uid, { active: true }); await refresh(); } catch (reason) { fail(reason); } }
  async function fieldArchived(){const name=archiveTarget?.label||'Field';await refresh();if(error){const reason=error;error='';throw new Error(`${name} is archived, but the settings could not be refreshed. Check again to confirm its status. ${reason}`);}archiveTarget=null;notice=`${name} archived. Historical values were preserved.`;}
  async function configureRules(field:FieldDefinition){if(moduleWorkspaceDirty&&!await requestConfirmation({title:'Discard unsaved module settings?',description:'Save module, field and storage settings first, or discard them before configuring rules.',confirmLabel:'Discard and configure',tone:'danger'}))return;syncModuleEditor();await refresh();const saved=schema?.fields.find(f=>f.uid===field.uid);if(saved)rulesTarget=cloneJson(saved);}
  async function rulesSaved(){await refresh();if(error){const message=error;error='';throw new Error(`Rules saved, but settings could not refresh: ${message}`);}notice='Form rules saved. Test the choices in a new record.';}
  async function openArchiveDependency(blocker:ArchiveBlocker){
    archiveTarget=null;
    if(blocker.section==='records'){
      if(moduleWorkspaceDirty&&!await requestConfirmation({title:'Discard unsaved settings?',description:'Opening records will discard unsaved module, field and storage settings.',confirmLabel:'Discard and open',tone:'danger'}))return;
      location.hash=`module/${encodeURIComponent(blocker.module_uid)}`;return;
    }
    await switchModule(blocker.module_uid);
    if(selectedModuleUid!==blocker.module_uid)return;
    section=blocker.section==='storage'?'storage':blocker.section==='workflow'?'workflow':blocker.section==='views'?'views':'fields';
    if(blocker.section==='workflow'||blocker.section==='views')return;
    if(blocker.kind==='form_rule'){const target=schema?.fields.find(f=>f.uid===blocker.field_uid);if(target)await configureRules(target);return;}
    fieldFocusUid=blocker.field_uid||'';await tick();
    const target=blocker.section==='storage'?document.querySelector<HTMLElement>('.storage-layout-panel'):document.querySelector<HTMLElement>(`[data-schema-field="${CSS.escape(fieldFocusUid)}"]`);
    target?.scrollIntoView({block:'center',behavior:'smooth'});target?.focus({preventScroll:true});
  }
  function schemaOrderFor(fieldUids: string[]) { if (!schema) return []; if (selectedModuleUid !== 'mx-default-records') return fieldUids.map((id) => ({ kind: 'field', id })); let fieldIndex = 0; return [...schema.system_fields.map((item) => ({ kind: 'system', id: item.key, position: item.position })), ...schema.fields.filter((item) => item.active).map((item) => ({ kind: 'field', id: item.uid, position: item.position }))].sort((a, b) => a.position - b.position).map((item) => item.kind === 'system' ? { kind: item.kind, id: item.id } : { kind: 'field', id: fieldUids[fieldIndex++] || item.id }); }
  async function saveOrder(fieldUids: string[]) { try { await updateSchemaOrder(schemaOrderFor(fieldUids), selectedModuleUid); await refresh(); notice = 'Field order saved.'; } catch (reason) { fail(reason); } }
  async function move(field: FieldDefinition, delta: number) { const ordered = [...activeFields]; const index = ordered.findIndex((item) => item.uid === field.uid); const target = index + delta; if (index < 0 || target < 0 || target >= ordered.length) return; [ordered[index], ordered[target]] = [ordered[target], ordered[index]]; await saveOrder(ordered.map((item) => item.uid)); }
  let draggedField = $state(''); let dragTargetField = $state('');
  function endFieldDrag() { draggedField = ''; dragTargetField = ''; }
  async function dropField(targetUid: string) { if (!draggedField || draggedField === targetUid) { endFieldDrag(); return; } const ordered = [...activeFields]; const from = ordered.findIndex((item) => item.uid === draggedField); const to = ordered.findIndex((item) => item.uid === targetUid); if (from < 0 || to < 0) { endFieldDrag(); return; } const [moved] = ordered.splice(from, 1); ordered.splice(to, 0, moved); endFieldDrag(); await saveOrder(ordered.map((item) => item.uid)); }
  async function saveSystem(keyName: string, visibleValue: boolean, priorityValue: number) { try { await updateSystemField(keyName, { table_visible: visibleValue, table_priority: priorityValue }); await refresh(); } catch (reason) { fail(reason); } }
  function fieldLabel(uid: string) { return activeFields.find((field) => field.uid === uid)?.label || 'Unavailable field'; }
  function addFolder() { if (!storageCandidate || folders.includes(storageCandidate)) return; folders = [...folders, storageCandidate]; storageCandidate = ''; }
  function moveFolder(index: number, delta: number) { const target = index + delta; if (target < 0 || target >= folders.length) return; const next = [...folders]; [next[index], next[target]] = [next[target], next[index]]; folders = next; }
  function removeFolder(index: number) { folders = folders.filter((_, item) => item !== index); }
  function stagedStoragePreview() { const parts = [storageRoot, ...folders.map((uid) => `<${fieldLabel(uid)}>`), '<File Attachment field>']; const prefixField = activeFields.find((field) => field.uid === prefix); parts.push(prefixField ? `<${prefixField.label}>__filename.ext` : 'filename.ext'); return parts.join('/'); }
  async function saveLayout() { try { const result = await saveStorageLayout(folders, prefix || null, selectedModuleUid); preview = result.preview; storageRoot = result.root; frozen = result.frozen_records; storageRevision = result.revision; storageConfigured = result.configured; savedFolders = [...folders]; savedPrefix = prefix; notice = `Independent N1 layout saved for ${result.module_name}.`; } catch (reason) { fail(reason); } }
</script>

{#if notice}<div class="notice success">{notice}</div>{/if}
{#if errorPopup}<SchemaErrorDialog title={errorPopup.title} message={errorPopup.message} conflict={errorPopup.conflict} onClose={()=>void closeError()} onUseKey={value=>void useAvailableKey(value)} onOpenField={conflict=>void viewKeyOwner(conflict)} />{/if}
{#if !selectedModule}
  <div class="module-workbench-heading"><div><p class="eyebrow">Workspace configuration</p><h2>Modules & fields</h2><p>Create a new module, or choose an existing module to configure. Nothing is selected automatically.</p></div><button class="button primary" type="button" disabled={loading} onclick={()=>void openModuleCreate()}><Plus size={16}/>New module</button></div>
  <section class="module-library" aria-label="Module library">
    <label class="module-library-search"><Search size={17}/><input type="search" aria-label="Find a module" bind:value={moduleSearch} placeholder="Find a module…"/></label>
    {#if loading}<p role="status">Loading modules…</p>{:else}<div class="module-library-grid">{#each matchingModules as module (module.uid)}<button class="module-library-card" type="button" aria-label={`Configure ${module.name}`} onclick={()=>void switchModule(module.uid)}><span class="module-library-icon" style={`--module-accent:${module.color}`}><ModuleIcon name={module.icon} size={24}/></span><span class="module-library-copy"><strong>{module.name}</strong><small>{module.description || 'No description yet'}</small><span>{module.active?'Active':'Archived'} · Configure module →</span></span></button>{:else}<p class="muted">{modules.length?'No modules match your search.':'No modules yet. Create your first module to begin.'}</p>{/each}</div>{/if}
  </section>
{/if}

{#if selectedModule}
<section class="module-workbench-scope" aria-label="Current module configuration scope"><button class="button small" type="button" disabled={loading} onclick={()=>void switchModule('')}><ArrowLeft size={16}/>All modules</button><span class="module-library-icon" style={`--module-accent:${selectedModule.color}`}><ModuleIcon name={selectedModule.icon} size={22}/></span><div><small>You are configuring</small><h2>{selectedModule.name}</h2></div><span class="scope-status">{moduleWorkspaceDirty?'Unsaved changes':'Existing module'}</span><button class="button" type="button" disabled={loading} onclick={()=>void openModuleCreate()}><Plus size={16}/>New module</button></section>
<nav class="module-section-nav" aria-label="Module configuration sections">{#each sections as item}<button type="button" class:active={section===item.id} aria-current={section===item.id?'page':undefined} onclick={()=>void switchSection(item.id)}>{item.name}</button>{/each}</nav>
<p class="module-section-description">{sections.find(item=>item.id===section)?.description}</p>
{#if loading}<p role="status">Loading {selectedModule.name} settings…</p>{/if}
<div class="module-section-content" inert={loading} aria-busy={loading}>
{#if section==='details'||section==='access'}
<section class="panel module-builder">
  <div class="panel-heading module-builder-heading">
    <div>
      <p class="eyebrow">Existing module</p>
      <h2>{section==='details'?'Module details':'Permissions'} · {selectedModule.name}</h2>
      <p class="muted">Only this module's identity, navigation, and access matrix are changed here.</p>
    </div>
    <span class:unsaved={moduleDirty} class="module-edit-state">{moduleDirty ? 'Unsaved module changes' : 'Module changes saved'}</span>
  </div>
  {#if selectedModule}
    <form onsubmit={saveModule}>
      {#if section==='access'}<div class="notice"><label>Who may see records?<select aria-label="Record visibility" bind:value={moduleRecordVisibility}><option value="module">Everyone with module access</option><option value="creator">Only the record creator and administrators</option></select></label><p>Module permissions still apply. Creator-only access also protects search, links, files, history, notifications and exports. Administrators retain access. Older records with unknown creators remain administrator-only; ownership is not guessed.</p></div>{/if}
      {#if section==='details'}
      <div class="module-editor">
        <label>Name<input bind:value={moduleName} /></label>
        <label>Singular name<input bind:value={moduleSingular} /></label>
        <label>Navigation icon<select bind:value={moduleIcon}>{#each MODULE_ICON_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}</select></label>
        <label>Color<input bind:value={moduleColor} type="color" /></label>
        <label>Navigation group<input bind:value={moduleNavigationGroup} maxlength="80" placeholder="e.g. Human Resources" /></label>
        <label>Navigation order<input bind:value={modulePosition} type="number" step="1" aria-label="Navigation order" aria-describedby="module-navigation-order-help" /><small id="module-navigation-order-help">Lower numbers appear first in the Modules menu and library. Use 10, 20, 30 to leave room between modules.</small></label>
        <label class="module-description">Description<input bind:value={moduleDescription} /></label>
        <label class="checkbox"><input type="checkbox" bind:checked={moduleActive} disabled={selectedModuleUid === 'mx-default-records'} /> Module is active</label>
        <div class="module-editor-actions">
          <button class="button" type="button" onclick={discardModuleChanges} disabled={loading || !moduleDirty}>Discard</button>
          <button class="button danger" type="button" onclick={beginModuleDelete} disabled={loading || selectedModuleUid === 'mx-default-records'} title={selectedModuleUid === 'mx-default-records' ? 'The core migrated module cannot be deleted' : `Delete ${selectedModule.name}`}><Trash2 size={15} /><span>Delete</span></button>
          <button class="button primary" type="submit" disabled={loading || !moduleDirty}>Save changes</button>
        </div>
      </div>
      {:else}
      <div class="module-permissions">
        <div class="permission-head"><strong>Access level</strong><span>View</span><span>Create</span><span>Edit</span><span>Delete</span><span>Configure</span><span>Reports</span><span>Files</span></div>
        {#each modulePermissions as permission}
          <div>
            <strong>{['Administrator','Manager','Editor','Viewer'][permission.access_level] || `Level ${permission.access_level}`}</strong>
            <label><input type="checkbox" bind:checked={permission.can_read} disabled={permission.access_level === 0} /><span>View</span></label>
            <label><input type="checkbox" bind:checked={permission.can_create} disabled={permission.access_level === 0} /><span>Create</span></label>
            <label><input type="checkbox" bind:checked={permission.can_update} disabled={permission.access_level === 0} /><span>Edit</span></label>
            <label><input type="checkbox" bind:checked={permission.can_delete} disabled={permission.access_level === 0} /><span>Delete</span></label>
            <label><input type="checkbox" bind:checked={permission.can_configure} disabled={permission.access_level === 0} /><span>Configure</span></label>
            <label><input type="checkbox" bind:checked={permission.can_report} disabled={permission.access_level === 0} /><span>Reports</span></label>
            <label><input type="checkbox" bind:checked={permission.can_attachments} disabled={permission.access_level === 0} /><span>Files</span></label>
          </div>
        {/each}
      </div>
      <div class="button-row"><button class="button" type="button" onclick={discardModuleChanges} disabled={loading||!moduleDirty}>Discard changes</button><button class="button primary" type="submit" disabled={loading||!moduleDirty}>Save permissions for {selectedModule.name}</button></div>
      {/if}
    </form>
  {/if}
</section>
{/if}
{#if schema&&section==='summaries'}<ColumnSummariesAdmin fields={schema.fields} moduleName={selectedModule?.name || 'Module'} onSaved={refresh} />{/if}
{#if schema&&section==='rules'}<FormRulesAdmin fields={schema.fields} moduleName={selectedModule?.name||'Module'} onConfigure={field=>void configureRules(field)}/>{/if}
{#if schema&&section==='workflow'}{#key selectedModuleUid}<WorkflowAdmin moduleUid={selectedModuleUid} moduleName={selectedModule.name} fields={schema.fields} bind:dirty={workflowDirty}/>{/key}{/if}
{#if schema&&section==='books'}{#key selectedModuleUid}<BooksSettings moduleUid={selectedModuleUid} moduleName={selectedModule.name} bind:dirty={booksDirty}/>{/key}{/if}
{#if schema&&section==='views'}{#key selectedModuleUid}<ModuleViewsAdmin moduleUid={selectedModuleUid} moduleName={selectedModule.name} fields={schema.fields} bind:dirty={viewsDirty}/>{/key}{/if}
<div class="admin-grid module-fields-grid">
{#if section==='fields'&&schema}
  <section class="panel module-section-panel"><div class="module-local-heading"><span>Module · {selectedModule?.name || 'Module'}</span><h2>Add a {selectedModule?.singular_name.toLowerCase() || 'record'} field</h2><p class="muted">The new field will exist only in {selectedModule?.name || 'this module'}.</p></div><form class="form-stack" onsubmit={add} aria-label="Add field" id="new-field-form" tabindex="-1"><label>Label<input bind:value={label} required oninput={()=>{createError='';createKeyInvalid=false;}} /></label><label>Field key (optional)<input id="new-field-key" bind:value={key} maxlength="64" pattern="[a-z_][a-z0-9_]*" placeholder={`Generated: ${generatedFieldKey(label)}`} aria-describedby="new-field-key-help" aria-invalid={createKeyInvalid} oninput={()=>{createError='';createKeyInvalid=false;}} /></label><p id="new-field-key-help" class="muted small">Stable identifier used by relationships and imports. Leave blank to generate it from the label. Active and archived fields must have different keys.</p>{#if (label.trim()||key.trim())&&keyOwner}<p class="field-key-warning" role="status">Key <code>{proposedKey}</code> is already reserved by <strong>{keyOwner.label}</strong> ({keyOwner.field_type}, {keyOwner.active?'active':'archived'}). Archiving keeps its key and history. Choose a different key for a new field.</p>{/if}{#if createError}<div class="notice error field-create-error" role="alert">{createError}</div>{/if}<label>Type<select bind:value={type} onchange={syncType}><option value="text">Text</option><option value="long_text">Long text</option><option value="integer">Integer</option><option value="decimal">Decimal</option><option value="formula">Formula / calculated</option><option value="relationship">Relationship / linked record</option><option value="lookup">Lookup from linked records</option><option value="rollup">Rollup / aggregate linked records</option><option value="date">Date</option><option value="boolean">Boolean</option><option value="select">Select</option><option value="auto_number">Auto number</option><option value="attachments">File attachments</option></select></label>{#if ['relationship','lookup','rollup'].includes(type)}<RelationshipConfig kind={type} bind:config={relationshipConfig} {modules} fields={activeFields} moduleUid={selectedModuleUid} />{:else if type === 'select'}<label>Options (comma separated)<input bind:value={options} required /></label>{:else if type === 'auto_number'}<label>Sequence scope<select bind:value={autoScope}><option value="global">Global</option><option value="yearly">Reset yearly</option></select></label><label>Prefix<input bind:value={autoPrefix} /></label><label>Zero padding<input type="number" min="0" max="12" bind:value={autoPadding} /></label>{:else if type === 'formula'}<FormulaConfig bind:config={formulaConfig} fields={activeFields}/>{:else if type === 'attachments'}<label>Storage name<input bind:value={attachmentStorage} placeholder="Defaults to field label" /></label><label>Description<input bind:value={attachmentDescription} /></label><label class="checkbox"><input type="checkbox" bind:checked={attachmentMultiple} /> Allow multiple files</label><label>Maximum files<input type="number" min="1" max="10000" bind:value={attachmentMaxFiles} disabled={!attachmentMultiple} placeholder="Unlimited" /></label>{/if}<label>Table priority<input type="number" bind:value={priority} /></label><label class="checkbox"><input type="checkbox" bind:checked={required} disabled={['auto_number', 'formula', 'lookup', 'rollup'].includes(type)} /> Required</label><label class="checkbox"><input type="checkbox" bind:checked={unique} disabled={['attachments', 'auto_number', 'formula', 'relationship', 'lookup', 'rollup'].includes(type)} /> No duplicate record values</label><label class="checkbox"><input type="checkbox" bind:checked={searchable} disabled={['relationship', 'lookup', 'rollup'].includes(type)} /> Searchable</label><label class="checkbox"><input type="checkbox" bind:checked={sortable} disabled={['long_text', 'attachments', 'relationship', 'lookup', 'rollup'].includes(type)} /> Sortable</label><label class="checkbox"><input type="checkbox" bind:checked={visible} /> Default table column</label><button class="button primary" disabled={loading}>Add field to {selectedModule?.name || 'module'}</button></form></section>
  <section class="panel admin-span"><div class="panel-heading"><div><p class="eyebrow">Module · {selectedModule?.name || 'Module'}</p><h2>{selectedModule?.name || 'Module'} record structure</h2><p class="muted">These fields drive only this module's record table, editor, reports, and N1 storage.</p></div><span class="security-pill">Schema revision {schema?.revision || 0}</span></div>
    <div class="schema-list">{#if selectedModuleUid === 'mx-default-records' && schema?.system_fields.length}<h3>Legacy system fields</h3>{#each schema.system_fields as field}<article class="schema-row"><div><strong>{field.label}</strong><small>{field.key} · {field.field_type}</small></div><label class="checkbox"><input type="checkbox" bind:checked={field.table_visible} /> Table</label><label>Priority<input class="short-input" type="number" bind:value={field.table_priority} /></label><button class="button small" onclick={() => saveSystem(field.key, field.table_visible, field.table_priority)}>Save</button></article>{/each}{/if}
      <div class="schema-section-heading"><h3>Custom fields</h3><small>Drag the grip to reorder active fields.</small></div>{#each schema?.fields || [] as field}<article data-schema-field={field.uid} tabindex="-1" class:dependency-focus={fieldFocusUid===field.uid} class:dragging={draggedField === field.uid} class:drop-target={dragTargetField === field.uid && draggedField !== field.uid} ondragover={(event) => { if (field.active && draggedField) { event.preventDefault(); dragTargetField = field.uid; } }} ondragleave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null) && dragTargetField === field.uid) dragTargetField = ''; }} ondrop={(event) => { event.preventDefault(); void dropField(field.uid); }} class:archived={!field.active} class="schema-card"><div class="schema-card-head"><div class="schema-field-title">{#if field.active}<button class="drag-handle" type="button" draggable="true" aria-label={`Drag ${field.label} to reorder`} title="Drag to reorder" ondragstart={(event) => { draggedField = field.uid; event.dataTransfer?.setData('text/plain', field.uid); }} ondragend={endFieldDrag}><GripVertical size={16} /></button>{/if}<div><strong>{field.label}</strong><small>{field.key} · {field.field_type}{field.active ? '' : ' · archived'}</small></div></div><div class="inline-actions"><button class="button small" aria-label="Move up" title="Move up" onclick={() => move(field, -1)} disabled={!field.active || activeFields[0]?.uid === field.uid}><ArrowUp size={15} /></button><button class="button small" aria-label="Move down" title="Move down" onclick={() => move(field, 1)} disabled={!field.active || activeFields[activeFields.length - 1]?.uid === field.uid}><ArrowDown size={15} /></button><button class="button small" onclick={() => toggle(field)}>{field.active ? 'Archive' : 'Restore'}</button></div></div><div class="schema-edit"><label>Datatype<select bind:value={field.field_type} aria-label={`Datatype for ${field.label}`} disabled={savedFieldTypes[field.uid]!=='text'}><option value={savedFieldTypes[field.uid]}>{savedFieldTypes[field.uid]?.replaceAll('_',' ') || field.field_type}</option>{#if savedFieldTypes[field.uid]==='text'}<option value="long_text">Long text</option>{/if}</select></label>{#if savedFieldTypes[field.uid]==='text'}<p class="muted small full">Text can become Long text in place. Existing records and history are kept; the key does not change.</p>{/if}<label>Label<input bind:value={field.label} /></label><label>Priority<input type="number" bind:value={field.table_priority} /></label><label>Field access<select aria-label={`Field access for ${field.label}`} value={String(field.config.access_scope||'module')} onchange={event=>field.config={...field.config,access_scope:event.currentTarget.value}}><option value="module">Everyone with module access</option><option value="creator">Record creator and administrators</option><option value="administrators">Administrators only</option></select></label><label class="checkbox"><input type="checkbox" bind:checked={field.required} disabled={['formula','lookup','rollup'].includes(field.field_type)} /> Required</label><label class="checkbox"><input type="checkbox" bind:checked={field.unique_value} disabled={['attachments', 'auto_number', 'formula', 'relationship', 'lookup', 'rollup'].includes(field.field_type)} /> No duplicate record values</label><label class="checkbox"><input type="checkbox" bind:checked={field.searchable} disabled={['relationship', 'lookup', 'rollup'].includes(field.field_type)} /> Searchable</label><label class="checkbox"><input type="checkbox" bind:checked={field.sortable} disabled={['attachments', 'long_text', 'relationship', 'lookup', 'rollup'].includes(field.field_type)} /> Sortable</label><label class="checkbox"><input type="checkbox" bind:checked={field.table_visible} /> Default table column</label>{#if ['relationship','lookup','rollup'].includes(field.field_type)}<RelationshipConfig kind={field.field_type} bind:config={field.config} {modules} fields={activeFields} moduleUid={selectedModuleUid} />{:else if field.field_type === 'select'}<label class="full">Options<input value={(field.config.options as JsonValue[] || []).join(', ')} oninput={(event) => field.config = { ...field.config, options: event.currentTarget.value.split(',').map((v) => v.trim()).filter(Boolean) }} /></label>{:else if field.field_type === 'auto_number'}<label>Scope<select value={String(field.config.scope || 'global')} onchange={(event) => field.config = { ...field.config, scope: event.currentTarget.value }}><option value="global">Global</option><option value="yearly">Reset yearly</option></select></label><label>Prefix<input value={String(field.config.prefix || '')} oninput={(event) => field.config = { ...field.config, prefix: event.currentTarget.value }} /></label><label>Padding<input type="number" min="0" max="12" value={Number(field.config.padding || 0)} oninput={(event) => field.config = { ...field.config, padding: Number(event.currentTarget.value) }} /></label>{:else if field.field_type === 'formula'}<FormulaConfig bind:config={field.config} fields={activeFields} fieldKey={field.key}/>{:else if field.field_type === 'attachments'}<label>Storage name<input value={String(field.config.storage_name || '')} oninput={(event) => field.config = { ...field.config, storage_name: event.currentTarget.value }} /></label><label>Description<input value={String(field.config.description || '')} oninput={(event) => field.config = { ...field.config, description: event.currentTarget.value }} /></label><label class="checkbox"><input type="checkbox" checked={field.config.multiple !== false} onchange={(event) => field.config = { ...field.config, multiple: event.currentTarget.checked }} /> Multiple files</label><label>Maximum files<input type="number" min="1" max="10000" value={String(field.config.max_files || '')} placeholder="Unlimited" oninput={(event) => { const raw = event.currentTarget.value; field.config = { ...field.config, max_files: raw ? Number(raw) : null }; }} /></label>{/if}<button class="button primary small" onclick={() => save(field)}>Save field</button></div></article>{/each}</div>
  </section>
{/if}
{#if section==='storage'&&schema}
  <section class="panel admin-wide storage-layout-panel"><div class="panel-heading"><div><p class="eyebrow">Module · {selectedModule?.name || 'Module'}</p><h2>{selectedModule?.name || 'Module'} N1 storage layout</h2><p class="muted">This folder structure applies only to {selectedModule?.name || 'the selected module'}. Return to All modules to configure another module.</p></div><span class:unsaved={storageDirty} class="module-edit-state">{storageDirty ? 'Unsaved storage changes' : `Storage revision ${storageRevision}`}</span></div><div class="storage-module-context"><div class="storage-module-identity"><i style={`--module-color:${selectedModule?.color || '#1d4ed8'}`}></i><span><strong>{selectedModule?.name || 'Module'} storage</strong><small>{storageConfigured ? 'Custom layout saved' : 'Using the module default'} · {frozen} frozen record folder{frozen === 1 ? '' : 's'}</small></span></div><div class="storage-root"><span>Independent N1 root</span><code>{storageRoot}/</code></div></div><div class="storage-builder"><div><label>Add a folder level<select bind:value={storageCandidate}><option value="">Choose a {selectedModule?.singular_name.toLowerCase() || 'record'} field…</option>{#each storageFields.filter((field) => !folders.includes(field.uid)) as field}<option value={field.uid}>{field.label}</option>{/each}</select></label><button class="button" type="button" onclick={addFolder} disabled={!storageCandidate}>Add level</button></div><ol class="storage-order">{#if !folders.length}<li class="storage-empty">Files for {selectedModule?.name || 'this module'} will be stored directly inside each record folder.</li>{/if}{#each folders as uid, index}<li><span class="storage-index">{index + 1}</span><div><strong>{fieldLabel(uid)}</strong><small>Folder level {index + 1}</small></div><div class="inline-actions"><button class="button small" type="button" aria-label="Move up" title="Move up" onclick={() => moveFolder(index, -1)} disabled={index === 0}><ArrowUp size={15} /></button><button class="button small" type="button" aria-label="Move down" title="Move down" onclick={() => moveFolder(index, 1)} disabled={index === folders.length - 1}><ArrowDown size={15} /></button><button class="button small danger" type="button" onclick={() => removeFolder(index)}>Remove</button></div></li>{/each}</ol></div><div class="storage-options"><label>File-name prefix<select bind:value={prefix}><option value="">Keep the uploaded file name</option>{#each storageFields as field}<option value={field.uid}>Prefix with {field.label}</option>{/each}</select></label><div class="storage-preview"><span>New {selectedModule?.singular_name.toLowerCase() || 'record'} path preview</span><code>{stagedStoragePreview()}</code>{#if preview}<small>Currently saved for {selectedModule?.name || 'this module'}: {preview}</small>{/if}</div></div><div class="button-row"><button class="button primary" onclick={saveLayout} disabled={loading || !storageDirty}>Save {selectedModule?.name || 'module'} storage layout</button></div></section>
{/if}
</div>
</div>

{:else if !loading}
<section class="panel empty-state">
  <h2>{modules.length?'Choose a module to configure':'Start with your own workflow'}</h2>
  <p>{modules.length?'Select Configure on a module above. To create something separate, use New module.':'No starter modules or business fields are supplied. Use New module, then add its fields.'}</p>
</section>
{/if}

{#if archiveTarget}{#key archiveTarget.uid}<FieldArchiveDialog fieldUid={archiveTarget.uid} fieldLabel={archiveTarget.label} onClose={()=>archiveTarget=null} onArchived={fieldArchived} onOpen={blocker=>void openArchiveDependency(blocker)}/>{/key}{/if}
{#if rulesTarget&&schema}{#key rulesTarget.uid}<FieldRulesDialog field={rulesTarget} fields={schema.fields} moduleName={selectedModule?.name||'Module'} onClose={()=>rulesTarget=null} onSaved={rulesSaved}/>{/key}{/if}

{#if moduleCreateOpen}
  <div class="overlay" use:dismissBackdrop={()=>void closeModuleCreate()}>
    <div class="dialog module-create-dialog" role="dialog" aria-modal="true" aria-labelledby="create-module-title" tabindex="-1" use:focusDialog onkeydown={event=>{if(event.key==='Escape'){event.preventDefault();event.stopPropagation();void closeModuleCreate();}}}>
      <header class="dialog-head">
        <div class="admin-dialog-title">
          <span><Plus size={19} /></span>
          <div>
            <p class="eyebrow">New module</p>
            <h2 id="create-module-title">Create a separate module</h2>
            <p>{selectedModule?.name || 'The current module'} will not be edited or duplicated.</p>
          </div>
        </div>
        <button class="icon-button" type="button" aria-label="Close new module form" onclick={closeModuleCreate} disabled={moduleCreateBusy}><X size={18} /></button>
      </header>
      <form class="module-create-form" onsubmit={addModule}>
        <div class="module-create-grid">
          <label>Module name<input bind:value={newModuleName} required placeholder="e.g. Cases" /></label>
          <label>Singular name<input bind:value={newModuleSingular} placeholder="e.g. Case" /></label>
          <label class="module-create-description">Description<input bind:value={newModuleDescription} placeholder="What this module manages" /></label>
          <label>Navigation icon<select bind:value={newModuleIcon}>{#each MODULE_ICON_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}</select></label>
          <label>Color<input bind:value={newModuleColor} type="color" /></label>
          <label class="module-create-navigation">Navigation group<input bind:value={newModuleNavigationGroup} maxlength="80" placeholder="e.g. Human Resources" /></label>
        </div>
        <div class="module-create-guidance">
          <strong>What happens next</strong>
          <span>MX creates an empty, active module and switches the configuration scope to it. You can then add its fields, permissions, and N1 storage layout.</span>
        </div>
        {#if moduleCreateError}<div class="notice error">{moduleCreateError}</div>{/if}
        <div class="dialog-actions">
          <button class="button" type="button" onclick={closeModuleCreate} disabled={moduleCreateBusy}>Cancel</button>
          <button class="button primary" disabled={moduleCreateBusy || !newModuleName.trim()}>{moduleCreateBusy ? 'Creating module…' : 'Create new module'}</button>
        </div>
      </form>
    </div>
  </div>
{/if}

{#if deletingModule}
  <div class="overlay" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) closeModuleDelete(); }}>
    <div class="dialog module-delete-dialog" role="alertdialog" aria-modal="true" aria-labelledby="delete-module-title">
      <header class="dialog-head"><div class="admin-dialog-title"><span><KeyRound size={19} /></span><div><p class="eyebrow">Administrator re-authentication</p><h2 id="delete-module-title">Delete {deletingModule.name}?</h2><p>This destructive operation is checked again by the MX server.</p></div></div><button class="icon-button" type="button" aria-label="Close module deletion" onclick={closeModuleDelete} disabled={deleteBusy}><X size={18} /></button></header>
      <form class="form-stack" onsubmit={confirmModuleDelete}>
        <div class="module-delete-warning"><ShieldAlert size={19} /><div><strong>This cannot be undone in MX.</strong>All records, history, schema fields, permissions, and dashboard widgets for this module will be removed. Attachment objects are moved to N1 trash before database deletion.</div></div>
        <label><span class="module-delete-confirm-label">Type <strong>{deletingModule.name}</strong> to confirm</span><input class="confirmation-input" bind:value={deleteConfirmation} autocomplete="off" required /></label>
        <label>Your administrator password<input bind:value={deletePassword} type="password" autocomplete="current-password" required /></label>
        {#if session.totp_enabled}<label>Authenticator or recovery code<input bind:value={deleteFactor} autocomplete="one-time-code" inputmode="text" required /></label>{/if}
        {#if deleteError}<div class="notice error">{deleteError}</div>{/if}
        <div class="dialog-actions"><button class="button" type="button" onclick={closeModuleDelete} disabled={deleteBusy}>Cancel</button><button class="button danger" disabled={deleteBusy || deleteConfirmation.trim() !== deletingModule.name || !deletePassword || (session.totp_enabled && !deleteFactor.trim())}>{deleteBusy ? 'Deleting module…' : 'Authenticate and delete'}</button></div>
      </form>
    </div>
  </div>
{/if}

<style>
  .module-workbench-heading{display:flex;align-items:center;justify-content:space-between;gap:1rem;margin:0 0 1.25rem;}.module-workbench-heading>div{min-width:0;}.module-workbench-heading h2{font-size:1.5rem;}.module-workbench-heading p:last-child{margin:0;color:var(--muted);font-size:.9rem;line-height:1.5;}.module-workbench-heading>.button{flex-shrink:0;}
  .module-library{border:1px solid var(--line);background:var(--surface);border-radius:.8rem;padding:1.2rem;margin-bottom:1rem;}.module-library-search{display:flex;align-items:center;gap:.6rem;max-width:28rem;margin-bottom:1rem;}.module-library-search input{flex:1;min-width:0;}.module-library-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,18rem),1fr));gap:.8rem;}
  .module-library-card{display:flex;align-items:flex-start;gap:.9rem;padding:1.1rem;border:1px solid var(--line);border-radius:.65rem;text-align:left;background:var(--surface);color:var(--text);font:inherit;cursor:pointer;min-width:0;}.module-library-card:hover,.module-library-card:focus-visible{border-color:var(--primary);background:var(--surface-2);}.module-library-icon{display:grid;place-items:center;width:2.75rem;height:2.75rem;flex:0 0 auto;border-radius:.6rem;color:var(--module-accent);background:color-mix(in srgb,var(--module-accent) 12%,var(--surface));}.module-library-copy{display:grid;gap:.35rem;min-width:0;overflow-wrap:anywhere;}.module-library-copy strong{font-size:1rem;}.module-library-copy small{font-size:.8rem;color:var(--muted);font-weight:400;line-height:1.5;}.module-library-copy>span{font-size:.75rem;color:var(--primary-text);margin-top:.4rem;}
  .module-workbench-scope{display:flex;align-items:center;gap:.8rem;padding:1rem;border:1px solid var(--line);border-radius:.75rem;background:var(--surface);}.module-workbench-scope>div{flex:1;min-width:0;}.module-workbench-scope h2{margin:0;font-size:1.2rem;overflow-wrap:anywhere;}.module-workbench-scope small,.scope-status{font-size:.75rem;color:var(--muted);}.scope-status{margin-left:auto;}
  .module-section-nav{display:flex;gap:.3rem;flex-wrap:wrap;border-bottom:1px solid var(--line);margin-top:1rem;padding-bottom:.45rem;}.module-section-nav button{font:inherit;font-size:.875rem;padding:.65rem .85rem;border:1px solid transparent;border-radius:.4rem;background:transparent;color:var(--muted);cursor:pointer;}.module-section-nav button:hover{background:var(--surface-2);color:var(--text);}.module-section-nav button.active{background:color-mix(in srgb,var(--primary) 10%,var(--surface));border-color:color-mix(in srgb,var(--primary) 35%,var(--line));color:var(--primary-text);font-weight:650;}.module-section-description{font-size:.85rem;color:var(--muted);line-height:1.5;margin:1rem 0;}
  .module-fields-grid{grid-template-columns:minmax(17rem,22rem) minmax(0,1fr);align-items:start;}.module-fields-grid>.admin-span{grid-column:auto;}.module-section-content{min-width:0;container-type:inline-size;}.module-section-content[aria-busy=true]{opacity:.5;}.module-fields-grid .storage-layout-panel{grid-column:1/-1;}
  @container(max-width:60rem){.module-fields-grid{grid-template-columns:minmax(0,1fr);}}
  .schema-card-head{flex-wrap:wrap;}.schema-field-title{min-width:0;overflow-wrap:anywhere;}.module-fields-grid :global(label){overflow-wrap:anywhere;}.module-fields-grid :global(.button){white-space:normal;max-width:100%;overflow-wrap:anywhere;}
  .module-fields-grid .panel-heading{flex-wrap:wrap;}.module-fields-grid .security-pill{max-width:100%;white-space:normal;overflow-wrap:anywhere;}
  .module-workbench-scope,.module-workbench-heading{flex-wrap:wrap;}
  .schema-edit{grid-template-columns:repeat(auto-fit,minmax(min(100%,14rem),1fr));}
  .module-fields-grid :global(input:not([type=checkbox])),.module-fields-grid :global(select){min-width:0;max-width:100%;width:100%;overflow:hidden;}
  @media(max-width:1100px){.module-fields-grid{grid-template-columns:minmax(0,1fr);}}@media(max-width:600px){.module-workbench-heading{align-items:flex-start;flex-direction:column;}.module-workbench-scope{flex-wrap:wrap;}.scope-status{width:100%;margin:0;}.module-workbench-scope>.button{width:100%;justify-content:flex-start;}.module-library{padding:.8rem;}}
  .field-key-warning{margin:0;padding:.75rem;border:1px solid var(--line);border-radius:.5rem;background:var(--surface-2);font-size:.85rem;overflow-wrap:anywhere}
  .field-create-error{margin:0;overflow-wrap:anywhere}
</style>
