<script lang="ts">
  import { onMount } from 'svelte';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import GripVertical from '@lucide/svelte/icons/grip-vertical';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import Plus from '@lucide/svelte/icons/plus';
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import type { FieldDefinition, JsonValue, ModuleDefinition, ModulePermission, SchemaResponse } from '../../api/domain';
  import type { Session } from '../../api/types';
  import { createField, createModule, deleteModule, loadModules, loadModuleSchema, loadSchema, loadStorageLayout, saveStorageLayout, updateField, updateModule, updateSchemaOrder, updateSystemField } from '../../api/workspace';
  import { cloneJson } from '../../util/json';
  import { requestConfirmation } from '../../confirmation';
  import { MODULE_ICON_OPTIONS, normaliseModuleIcon } from '../../util/moduleIcons';

  let { session }: { session: Session } = $props();

  let schema = $state<SchemaResponse | null>(null); let loading = $state(true); let error = $state(''); let notice = $state('');
  let label = $state(''); let key = $state(''); let type = $state<FieldDefinition['field_type']>('text'); let required = $state(false); let unique = $state(false); let searchable = $state(true); let sortable = $state(true); let visible = $state(true); let priority = $state(50); let options = $state('');
  let autoScope = $state('global'); let autoPrefix = $state(''); let autoPadding = $state(0);
  let formulaExpression = $state(''); let formulaDecimals = $state(2);
  let attachmentStorage = $state(''); let attachmentDescription = $state(''); let attachmentMultiple = $state(true); let attachmentMaxFiles = $state<number | undefined>(undefined);
  let folders = $state<string[]>([]); let storageCandidate = $state(''); let prefix = $state(''); let preview = $state(''); let storageRoot = $state('records'); let frozen = $state(0); let storageRevision = $state(0); let storageConfigured = $state(false);
  let savedFolders = $state<string[]>([]); let savedPrefix = $state('');
  let modules = $state<ModuleDefinition[]>([]); let selectedModuleUid = $state('mx-default-records');
  let moduleName = $state(''); let moduleSingular = $state(''); let moduleDescription = $state(''); let moduleIcon = $state('file-text'); let moduleColor = $state('#1d4ed8');
  let modulePosition = $state(0); let moduleActive = $state(true); let moduleNavigationGroup = $state('');
  let modulePermissions = $state<ModulePermission[]>([]);
  let moduleCreateOpen = $state(false); let moduleCreateBusy = $state(false); let moduleCreateError = $state('');
  let newModuleName = $state(''); let newModuleSingular = $state(''); let newModuleDescription = $state(''); let newModuleIcon = $state('file-text'); let newModuleColor = $state('#3768a6'); let newModuleNavigationGroup = $state('');
  let deletingModule = $state<ModuleDefinition | null>(null); let deleteConfirmation = $state(''); let deletePassword = $state(''); let deleteFactor = $state(''); let deleteError = $state(''); let deleteBusy = $state(false);
  const activeFields = $derived(schema?.fields.filter((field) => field.active).sort((a, b) => a.position - b.position) || []);
  const storageFields = $derived(activeFields.filter((field) => field.field_type !== 'attachments'));
  const selectedModule = $derived(modules.find((module) => module.uid === selectedModuleUid) || null);
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
      || moduleNavigationGroup !== savedNavigationGroup
      || JSON.stringify(modulePermissions) !== JSON.stringify(selectedModule.permissions);
  });
  const storageDirty = $derived(JSON.stringify(folders) !== JSON.stringify(savedFolders) || prefix !== savedPrefix);
  const moduleWorkspaceDirty = $derived(moduleDirty || storageDirty);

  onMount(() => void initialise());
  async function initialise() { try { modules = (await loadModules()).modules; selectedModuleUid = modules.find((module) => module.uid === selectedModuleUid)?.uid || modules[0]?.uid || 'mx-default-records'; syncModuleEditor(); await refresh(); } catch (reason) { fail(reason); loading = false; } }
  function syncModuleEditor() { const module = modules.find((item) => item.uid === selectedModuleUid); if (!module) return; moduleName = module.name; moduleSingular = module.singular_name; moduleDescription = module.description; moduleIcon = normaliseModuleIcon(module.icon); moduleColor = module.color; modulePosition = module.position; moduleActive = module.active; moduleNavigationGroup = typeof module.config.navigation_group === 'string' ? module.config.navigation_group : ''; modulePermissions = cloneJson(module.permissions); }
  async function switchModule(nextUid: string, control?: HTMLSelectElement) {
    if (!nextUid || nextUid === selectedModuleUid) return;
    if (moduleWorkspaceDirty && !await requestConfirmation({
      title: `Discard unsaved changes to ${selectedModule?.name || 'this module'}?`,
      description: `You are switching to ${modules.find((module) => module.uid === nextUid)?.name || 'another module'}. Unsaved module or N1 storage changes will be lost.`,
      confirmLabel: 'Discard and switch',
      tone: 'danger'
    })) {
      if (control) control.value = selectedModuleUid;
      return;
    }
    selectedModuleUid = nextUid;
    syncModuleEditor();
    notice = '';
    await refresh();
  }
  function generatedFieldKey() { return label.trim().toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '').replace(/^[0-9]+/, '') || 'field'; }
  async function refresh() { loading = true; error = ''; try { schema = selectedModuleUid === 'mx-default-records' ? await loadSchema(true) : await loadModuleSchema(selectedModuleUid); const layout = await loadStorageLayout(selectedModuleUid); folders = layout.folder_fields.map((field) => field.uid); prefix = layout.file_prefix_field?.uid || ''; savedFolders = [...folders]; savedPrefix = prefix; preview = layout.preview; storageRoot = layout.root; frozen = layout.frozen_records; storageRevision = layout.revision; storageConfigured = layout.configured; } catch (reason) { fail(reason); } finally { loading = false; } }
  function fail(reason: unknown) { error = reason instanceof Error ? reason.message : 'The operation failed.'; notice = ''; }
  function configFor(fieldType: string, currentOptions = ''): Record<string, JsonValue> { if (fieldType === 'select') return { options: currentOptions.split(',').map((v) => v.trim()).filter(Boolean) }; if (fieldType === 'attachments') { const maximum = Number(attachmentMaxFiles); return { storage_name: attachmentStorage || key || label, description: attachmentDescription, multiple: attachmentMultiple, max_files: attachmentMultiple && Number.isFinite(maximum) && maximum > 0 ? maximum : attachmentMultiple ? null : 1 }; } if (fieldType === 'auto_number') return { scope: autoScope, prefix: autoPrefix, padding: autoPadding }; if (fieldType === 'formula') return { expression: formulaExpression.trim(), decimals: formulaDecimals }; return {}; }
  function syncType() { if (type === 'auto_number') { required = true; unique = true; sortable = true; } else if (type === 'formula') { required = false; unique = false; searchable = true; sortable = true; } else if (type === 'attachments') { unique = false; searchable = true; sortable = false; } else if (type === 'long_text') sortable = false; else sortable = true; }
  async function add(event: SubmitEvent) { event.preventDefault(); loading = true; try { await createField({ label, key: key.trim().toLowerCase() || generatedFieldKey(), field_type: type, required: type === 'auto_number' ? true : type === 'formula' ? false : required, unique_value: type === 'auto_number' ? true : ['attachments', 'formula'].includes(type) ? false : unique, searchable, sortable: ['long_text', 'attachments'].includes(type) ? false : sortable, table_visible: visible, table_priority: priority, config: configFor(type, options) }, selectedModuleUid); label = ''; key = ''; type = 'text'; options = ''; autoScope = 'global'; autoPrefix = ''; autoPadding = 0; formulaExpression = ''; formulaDecimals = 2; attachmentStorage = ''; attachmentDescription = ''; attachmentMultiple = true; attachmentMaxFiles = undefined; required = false; unique = false; searchable = true; sortable = true; notice = 'Field created.'; await refresh(); } catch (reason) { fail(reason); } finally { loading = false; } }
  function resetNewModule() { newModuleName = ''; newModuleSingular = ''; newModuleDescription = ''; newModuleIcon = 'file-text'; newModuleColor = '#3768a6'; newModuleNavigationGroup = ''; moduleCreateError = ''; }
  async function openModuleCreate() {
    if (moduleWorkspaceDirty && !await requestConfirmation({ title: `Discard unsaved changes to ${selectedModule?.name || 'this module'}?`, description: 'The new-module workflow is separate from the current editor. Save your module and N1 layout changes first, or discard them before continuing.', confirmLabel: 'Discard and create new', tone: 'danger' })) return;
    if (moduleDirty) syncModuleEditor();
    if (storageDirty) { folders = [...savedFolders]; prefix = savedPrefix; }
    resetNewModule();
    moduleCreateOpen = true;
  }
  function closeModuleCreate() { if (moduleCreateBusy) return; moduleCreateOpen = false; resetNewModule(); }
  async function addModule(event: SubmitEvent) {
    event.preventDefault(); moduleCreateBusy = true; moduleCreateError = '';
    try {
      const result = await createModule({ name: newModuleName, singular_name: newModuleSingular || undefined, description: newModuleDescription, icon: newModuleIcon, color: newModuleColor, config: { navigation_group: newModuleNavigationGroup.trim() } });
      modules = [...modules, result.module].sort((a,b) => a.position-b.position); selectedModuleUid = result.module.uid; moduleCreateOpen = false; resetNewModule(); syncModuleEditor(); notice = `${result.module.name} created. You are now editing the new module; add its fields below.`; await refresh();
    } catch (reason) { moduleCreateError = reason instanceof Error ? reason.message : 'The module could not be created.'; }
    finally { moduleCreateBusy = false; }
  }
  async function saveModule(event?: SubmitEvent) { event?.preventDefault(); if (!selectedModule || !moduleDirty) return; loading = true; try { const result = await updateModule(selectedModule.uid, { name: moduleName, singular_name: moduleSingular, description: moduleDescription, icon: moduleIcon, color: moduleColor, position: modulePosition, active: moduleActive, config: { ...selectedModule.config, navigation_group: moduleNavigationGroup.trim() }, permissions: modulePermissions }); modules = modules.map((module) => module.uid === result.module.uid ? result.module : module).sort((a,b) => a.position-b.position); syncModuleEditor(); notice = 'Module identity, navigation, and access matrix saved.'; } catch (reason) { fail(reason); } finally { loading = false; } }
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
      selectedModuleUid = modules.find((module) => module.uid === 'mx-default-records')?.uid || modules[0]?.uid || 'mx-default-records';
      deletingModule = null; syncModuleEditor();
      notice = `${result.module_name} deleted. ${result.records_deleted} record${result.records_deleted === 1 ? '' : 's'} removed${result.attachments_moved_to_n1_trash ? `; ${result.attachments_moved_to_n1_trash} attachment${result.attachments_moved_to_n1_trash === 1 ? '' : 's'} moved to N1 trash` : ''}.`;
      error = ''; await refresh();
    } catch (reason) { deleteError = reason instanceof Error ? reason.message : 'The module could not be deleted.'; }
    finally { deleteBusy = false; }
  }
  async function save(field: FieldDefinition) { try { await updateField(field.uid, { label: field.label, required: field.field_type === 'auto_number' ? true : field.required, unique_value: field.field_type === 'auto_number' ? true : field.field_type === 'attachments' ? false : field.unique_value, searchable: field.searchable, sortable: ['long_text', 'attachments'].includes(field.field_type) ? false : field.sortable, table_visible: field.table_visible, table_priority: field.table_priority, config: field.config }); notice = `${field.label} saved.`; await refresh(); } catch (reason) { fail(reason); } }
  async function toggle(field: FieldDefinition) { const action = field.active ? 'Archive' : 'Restore'; if (!await requestConfirmation({ title: `${action} ${field.label}?`, description: 'Historical values will be preserved.', confirmLabel: `${action} field`, tone: field.active ? 'danger' : 'primary' })) return; try { await updateField(field.uid, { active: !field.active }); await refresh(); } catch (reason) { fail(reason); } }
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

{#if notice}<div class="notice success">{notice}</div>{/if}{#if error}<div class="notice error">{error}</div>{/if}
{#if selectedModule}
  <section class="module-scope-bar" aria-label="Current module configuration scope">
    <div class="module-scope-identity">
      <i style={`--module-color:${selectedModule.color}`}></i>
      <span>
        <small>Module in scope</small>
        <strong>{selectedModule.name}</strong>
      </span>
    </div>
    <p>Identity, access, fields, record structure, and N1 storage below all belong to this module.</p>
    <label>
      Change module
      <select value={selectedModuleUid} onchange={(event) => void switchModule(event.currentTarget.value, event.currentTarget)} disabled={loading}>
        {#each modules as module}<option value={module.uid}>{module.name}{module.active ? '' : ' (archived)'}</option>{/each}
      </select>
    </label>
    <button class="button primary module-new-button" type="button" onclick={() => void openModuleCreate()} disabled={loading}>
      <Plus size={15} />
      <span>New module</span>
    </button>
  </section>
{/if}

<section class="panel module-builder">
  <div class="panel-heading module-builder-heading">
    <div>
      <p class="eyebrow">Existing module</p>
      <h2>Edit {selectedModule?.name || 'module'}</h2>
      <p class="muted">Only this module's identity, navigation, and access matrix are changed here.</p>
    </div>
    <span class:unsaved={moduleDirty} class="module-edit-state">{moduleDirty ? 'Unsaved module changes' : 'Module changes saved'}</span>
  </div>
  {#if selectedModule}
    <form onsubmit={saveModule}>
      <div class="module-editor">
        <label>Name<input bind:value={moduleName} /></label>
        <label>Singular name<input bind:value={moduleSingular} /></label>
        <label>Navigation icon<select bind:value={moduleIcon}>{#each MODULE_ICON_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}</select></label>
        <label>Color<input bind:value={moduleColor} type="color" /></label>
        <label>Navigation group<input bind:value={moduleNavigationGroup} maxlength="80" placeholder="e.g. Human Resources" /></label>
        <label>Navigation order<input bind:value={modulePosition} type="number" /></label>
        <label class="module-description">Description<input bind:value={moduleDescription} /></label>
        <label class="checkbox"><input type="checkbox" bind:checked={moduleActive} disabled={selectedModuleUid === 'mx-default-records'} /> Module is active</label>
        <div class="module-editor-actions">
          <button class="button" type="button" onclick={discardModuleChanges} disabled={loading || !moduleDirty}>Discard</button>
          <button class="button danger" type="button" onclick={beginModuleDelete} disabled={loading || selectedModuleUid === 'mx-default-records'} title={selectedModuleUid === 'mx-default-records' ? 'The core migrated module cannot be deleted' : `Delete ${selectedModule.name}`}><Trash2 size={15} /><span>Delete</span></button>
          <button class="button primary" type="submit" disabled={loading || !moduleDirty}>Save changes</button>
        </div>
      </div>
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
    </form>
  {/if}
</section>
<div class="admin-grid">
  <section class="panel module-section-panel"><div class="module-local-heading"><span>Module · {selectedModule?.name || 'Module'}</span><h2>Add a {selectedModule?.singular_name.toLowerCase() || 'record'} field</h2><p class="muted">The new field will exist only in {selectedModule?.name || 'this module'}.</p></div><form class="form-stack" onsubmit={add}><label>Label<input bind:value={label} required /></label><label>Key (optional override)<input bind:value={key} pattern="[a-z_][a-z0-9_]*" placeholder={`Generated: ${generatedFieldKey()}`} /></label><label>Type<select bind:value={type} onchange={syncType}><option value="text">Text</option><option value="long_text">Long text</option><option value="integer">Integer</option><option value="decimal">Decimal</option><option value="formula">Formula / calculated</option><option value="date">Date</option><option value="boolean">Boolean</option><option value="select">Select</option><option value="auto_number">Auto number</option><option value="attachments">File attachments</option></select></label>{#if type === 'select'}<label>Options (comma separated)<input bind:value={options} required /></label>{:else if type === 'auto_number'}<label>Sequence scope<select bind:value={autoScope}><option value="global">Global</option><option value="yearly">Reset yearly</option></select></label><label>Prefix<input bind:value={autoPrefix} /></label><label>Zero padding<input type="number" min="0" max="12" bind:value={autoPadding} /></label>{:else if type === 'formula'}<label>Formula expression<input bind:value={formulaExpression} required placeholder="value * khw" /></label><label>Decimal places<input type="number" min="0" max="12" bind:value={formulaDecimals} /></label><p class="muted small">Use field keys with +, −, *, /, %, ^ and functions such as SUM, AVERAGE, MIN, MAX, ROUND, ABS, SQRT, POWER, MOD, and CLAMP.</p>{:else if type === 'attachments'}<label>Storage name<input bind:value={attachmentStorage} placeholder="Defaults to field label" /></label><label>Description<input bind:value={attachmentDescription} /></label><label class="checkbox"><input type="checkbox" bind:checked={attachmentMultiple} /> Allow multiple files</label><label>Maximum files<input type="number" min="1" max="10000" bind:value={attachmentMaxFiles} disabled={!attachmentMultiple} placeholder="Unlimited" /></label>{/if}<label>Table priority<input type="number" bind:value={priority} /></label><label class="checkbox"><input type="checkbox" bind:checked={required} disabled={['auto_number', 'formula'].includes(type)} /> Required</label><label class="checkbox"><input type="checkbox" bind:checked={unique} disabled={['attachments', 'auto_number', 'formula'].includes(type)} /> Unique</label><label class="checkbox"><input type="checkbox" bind:checked={searchable} /> Searchable</label><label class="checkbox"><input type="checkbox" bind:checked={sortable} disabled={['long_text', 'attachments'].includes(type)} /> Sortable</label><label class="checkbox"><input type="checkbox" bind:checked={visible} /> Default table column</label><button class="button primary" disabled={loading}>Add field to {selectedModule?.name || 'module'}</button></form></section>
  <section class="panel admin-span"><div class="panel-heading"><div><p class="eyebrow">Module · {selectedModule?.name || 'Module'}</p><h2>{selectedModule?.name || 'Module'} record structure</h2><p class="muted">These fields drive only this module's record table, editor, reports, and N1 storage.</p></div><span class="security-pill">Schema revision {schema?.revision || 0}</span></div>
    <div class="schema-list">{#if selectedModuleUid === 'mx-default-records' && schema?.system_fields.length}<h3>Legacy system fields</h3>{#each schema.system_fields as field}<article class="schema-row"><div><strong>{field.label}</strong><small>{field.key} · {field.field_type}</small></div><label class="checkbox"><input type="checkbox" bind:checked={field.table_visible} /> Table</label><label>Priority<input class="short-input" type="number" bind:value={field.table_priority} /></label><button class="button small" onclick={() => saveSystem(field.key, field.table_visible, field.table_priority)}>Save</button></article>{/each}{/if}
      <div class="schema-section-heading"><h3>Custom fields</h3><small>Drag the grip to reorder active fields.</small></div>{#each schema?.fields || [] as field}<article class:dragging={draggedField === field.uid} class:drop-target={dragTargetField === field.uid && draggedField !== field.uid} ondragover={(event) => { if (field.active && draggedField) { event.preventDefault(); dragTargetField = field.uid; } }} ondragleave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null) && dragTargetField === field.uid) dragTargetField = ''; }} ondrop={(event) => { event.preventDefault(); void dropField(field.uid); }} class:archived={!field.active} class="schema-card"><div class="schema-card-head"><div class="schema-field-title">{#if field.active}<button class="drag-handle" type="button" draggable="true" aria-label={`Drag ${field.label} to reorder`} title="Drag to reorder" ondragstart={(event) => { draggedField = field.uid; event.dataTransfer?.setData('text/plain', field.uid); }} ondragend={endFieldDrag}><GripVertical size={16} /></button>{/if}<div><strong>{field.label}</strong><small>{field.key} · {field.field_type}{field.active ? '' : ' · archived'}</small></div></div><div class="inline-actions"><button class="button small" aria-label="Move up" title="Move up" onclick={() => move(field, -1)} disabled={!field.active || activeFields[0]?.uid === field.uid}><ArrowUp size={15} /></button><button class="button small" aria-label="Move down" title="Move down" onclick={() => move(field, 1)} disabled={!field.active || activeFields[activeFields.length - 1]?.uid === field.uid}><ArrowDown size={15} /></button><button class="button small" onclick={() => toggle(field)}>{field.active ? 'Archive' : 'Restore'}</button></div></div><div class="schema-edit"><label>Label<input bind:value={field.label} /></label><label>Priority<input type="number" bind:value={field.table_priority} /></label><label class="checkbox"><input type="checkbox" bind:checked={field.required} disabled={field.field_type === 'formula'} /> Required</label><label class="checkbox"><input type="checkbox" bind:checked={field.unique_value} disabled={['attachments', 'auto_number', 'formula'].includes(field.field_type)} /> Unique</label><label class="checkbox"><input type="checkbox" bind:checked={field.searchable} /> Searchable</label><label class="checkbox"><input type="checkbox" bind:checked={field.sortable} disabled={['attachments', 'long_text'].includes(field.field_type)} /> Sortable</label><label class="checkbox"><input type="checkbox" bind:checked={field.table_visible} /> Default table column</label>{#if field.field_type === 'select'}<label class="full">Options<input value={(field.config.options as JsonValue[] || []).join(', ')} oninput={(event) => field.config = { options: event.currentTarget.value.split(',').map((v) => v.trim()).filter(Boolean) }} /></label>{:else if field.field_type === 'auto_number'}<label>Scope<select value={String(field.config.scope || 'global')} onchange={(event) => field.config = { ...field.config, scope: event.currentTarget.value }}><option value="global">Global</option><option value="yearly">Reset yearly</option></select></label><label>Prefix<input value={String(field.config.prefix || '')} oninput={(event) => field.config = { ...field.config, prefix: event.currentTarget.value }} /></label><label>Padding<input type="number" min="0" max="12" value={Number(field.config.padding || 0)} oninput={(event) => field.config = { ...field.config, padding: Number(event.currentTarget.value) }} /></label>{:else if field.field_type === 'formula'}<label class="full">Formula expression<input value={String(field.config.expression || '')} oninput={(event) => field.config = { ...field.config, expression: event.currentTarget.value }} placeholder="value * khw" /></label><label>Decimal places<input type="number" min="0" max="12" value={Number(field.config.decimals ?? 2)} oninput={(event) => field.config = { ...field.config, decimals: Number(event.currentTarget.value) }} /></label><p class="muted small full">Field keys are recalculated whenever the record changes.</p>{:else if field.field_type === 'attachments'}<label>Storage name<input value={String(field.config.storage_name || '')} oninput={(event) => field.config = { ...field.config, storage_name: event.currentTarget.value }} /></label><label>Description<input value={String(field.config.description || '')} oninput={(event) => field.config = { ...field.config, description: event.currentTarget.value }} /></label><label class="checkbox"><input type="checkbox" checked={field.config.multiple !== false} onchange={(event) => field.config = { ...field.config, multiple: event.currentTarget.checked }} /> Multiple files</label><label>Maximum files<input type="number" min="1" max="10000" value={String(field.config.max_files || '')} placeholder="Unlimited" oninput={(event) => { const raw = event.currentTarget.value; field.config = { ...field.config, max_files: raw ? Number(raw) : null }; }} /></label>{/if}<button class="button primary small" onclick={() => save(field)}>Save field</button></div></article>{/each}</div>
  </section>
  <section class="panel admin-wide storage-layout-panel"><div class="panel-heading"><div><p class="eyebrow">Module · {selectedModule?.name || 'Module'}</p><h2>{selectedModule?.name || 'Module'} N1 storage layout</h2><p class="muted">This folder structure applies only to {selectedModule?.name || 'the selected module'}. Change modules with the scope control above.</p></div><span class:unsaved={storageDirty} class="module-edit-state">{storageDirty ? 'Unsaved storage changes' : `Storage revision ${storageRevision}`}</span></div><div class="storage-module-context"><div class="storage-module-identity"><i style={`--module-color:${selectedModule?.color || '#1d4ed8'}`}></i><span><strong>{selectedModule?.name || 'Module'} storage</strong><small>{storageConfigured ? 'Custom layout saved' : 'Using the module default'} · {frozen} frozen record folder{frozen === 1 ? '' : 's'}</small></span></div><div class="storage-root"><span>Independent N1 root</span><code>{storageRoot}/</code></div></div><div class="storage-builder"><div><label>Add a folder level<select bind:value={storageCandidate}><option value="">Choose a {selectedModule?.singular_name.toLowerCase() || 'record'} field…</option>{#each storageFields.filter((field) => !folders.includes(field.uid)) as field}<option value={field.uid}>{field.label}</option>{/each}</select></label><button class="button" type="button" onclick={addFolder} disabled={!storageCandidate}>Add level</button></div><ol class="storage-order">{#if !folders.length}<li class="storage-empty">Files for {selectedModule?.name || 'this module'} will be stored directly inside each record folder.</li>{/if}{#each folders as uid, index}<li><span class="storage-index">{index + 1}</span><div><strong>{fieldLabel(uid)}</strong><small>Folder level {index + 1}</small></div><div class="inline-actions"><button class="button small" type="button" aria-label="Move up" title="Move up" onclick={() => moveFolder(index, -1)} disabled={index === 0}><ArrowUp size={15} /></button><button class="button small" type="button" aria-label="Move down" title="Move down" onclick={() => moveFolder(index, 1)} disabled={index === folders.length - 1}><ArrowDown size={15} /></button><button class="button small danger" type="button" onclick={() => removeFolder(index)}>Remove</button></div></li>{/each}</ol></div><div class="storage-options"><label>File-name prefix<select bind:value={prefix}><option value="">Keep the uploaded file name</option>{#each storageFields as field}<option value={field.uid}>Prefix with {field.label}</option>{/each}</select></label><div class="storage-preview"><span>New {selectedModule?.singular_name.toLowerCase() || 'record'} path preview</span><code>{stagedStoragePreview()}</code>{#if preview}<small>Currently saved for {selectedModule?.name || 'this module'}: {preview}</small>{/if}</div></div><div class="button-row"><button class="button primary" onclick={saveLayout} disabled={loading || !storageDirty}>Save {selectedModule?.name || 'module'} storage layout</button></div></section>
</div>

{#if moduleCreateOpen}
  <div class="overlay" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) closeModuleCreate(); }}>
    <div class="dialog module-create-dialog" role="dialog" aria-modal="true" aria-labelledby="create-module-title">
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
