<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import FileText from '@lucide/svelte/icons/file-text';
  import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import Upload from '@lucide/svelte/icons/upload';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import { ApiError, ConnectionError } from '../../api/client';
  import type { AuditPage, BackupEntry, DeploymentConfig } from '../../api/domain';
  import { requestConfirmation } from '../../confirmation';
  import { createBackup, downloadBackup, listBackups, loadAudit, loadDeployment, saveDeployment, uploadDeploymentLogo, verifyBackup } from '../../api/workspace';

  let { section = 'deployment' }: { section?: 'deployment' | 'backups' | 'audit' } = $props();

  let config = $state<DeploymentConfig | null>(null); let configRevision = $state(0);
  let backups = $state<BackupEntry[]>([]); let backupRunning = $state(false);
  let backupPending = $state(false); let backupWaiting = $state(false); let backupHistoryLoaded = $state(false);
  let backupStatusBusy = $state(false); let backupMessage = $state(''); let backupProblem = $state(''); let backupConnection = $state('');
  let backupStartedAt = $state(0); let backupElapsed = $state(0); let backupOwnStart = $state(false);
  const backupActive = $derived(backupPending || backupRunning || backupWaiting);
  let backupObservedRun = false; let backupUnconfirmed = false; let backupEmptyChecks = 0; let backupBaseline = new Set<string>(); let backupStatusSequence = 0;
  let backupStatusController: AbortController | undefined; let alive = true;
  let audit = $state<AuditPage | null>(null); let auditPage = $state(1); let auditUser = $state(''); let auditAction = $state(''); let auditResult = $state('all');
  let loading = $state(true); let busy = $state(''); let error = $state(''); let notice = $state('');
  let logoFile = $state<File | null>(null); let logoProgress = $state(0);
  const auditDay = $derived((audit?.data || []).filter((entry) => { const value = entry.created_at > 10_000_000_000 ? entry.created_at : entry.created_at * 1000; return value >= Date.now() - 86_400_000; }).length);
  const auditFailed = $derived((audit?.data || []).filter((entry) => !entry.success).length);
  const auditActors = $derived(new Set((audit?.data || []).map((entry) => entry.actor_uid).filter(Boolean)).size);

  onMount(() => { void refresh(); void refreshBackupStatus(); });
  onDestroy(() => { alive = false; ++backupStatusSequence; backupStatusController?.abort(); });
  $effect(() => {
    if (!backupActive || section !== 'backups') return;
    const poll = setInterval(() => void refreshBackupStatus(), 2000);
    const clock = setInterval(() => backupElapsed = Math.max(0, Math.floor((Date.now() - backupStartedAt) / 1000)), 1000);
    return () => { clearInterval(poll); clearInterval(clock); };
  });
  async function refresh() { loading = true; try { const [deployment, auditData] = await Promise.all([loadDeployment(), loadAudit(auditPage, auditUser, auditAction, auditResult)]); if (!alive) return; config = deployment.config; configRevision = deployment.revision; audit = auditData; } catch (reason) { if (alive) fail(reason); } finally { if (alive) loading = false; } }
  async function refreshBackupStatus() {
    if (!alive || backupStatusBusy) return;
    backupStatusBusy = true; const sequence = ++backupStatusSequence;
    const controller = backupStatusController = new AbortController();
    try {
      const result = await listBackups(controller.signal);
      if (!alive || sequence !== backupStatusSequence) return;
      if (!backupHistoryLoaded) backupBaseline = new Set(result.backups.map(item => item.uid));
      backups = result.backups; backupHistoryLoaded = true; backupConnection = ''; backupRunning = result.backup_running;
      if (backupRunning) {
        backupObservedRun = true; backupEmptyChecks = 0;
        if (!backupStartedAt) { backupStartedAt = Date.now(); backupElapsed = 0; backupOwnStart = false; }
      } else if (!backupPending && (backupObservedRun || backupWaiting)) {
        const completed = backups.find(item => !backupBaseline.has(item.uid) && item.status === 'verified');
        if (completed) { backupObservedRun = false; backupUnconfirmed = false; backupWaiting = false; backupMessage = `A new verified backup is available in N1: ${completed.file_name}`; backupProblem = ''; }
        // History and the running flag are not read atomically by the current
        // API. Recheck once before treating a missing completion as a failure.
        else if (++backupEmptyChecks < 2) backupWaiting = true;
        else { backupObservedRun = false; backupWaiting = false; if (backupUnconfirmed || !backupProblem) backupProblem = 'MX reports no backup running, but no new completed backup is recorded. Check the history before starting another backup.'; backupUnconfirmed = false; }
      }
    } catch (reason) {
      if (!alive || controller.signal.aborted || sequence !== backupStatusSequence) return;
      backupConnection = reason instanceof Error ? reason.message : 'Backup status could not be checked.';
      if (reason instanceof ApiError && [401,403].includes(reason.status)) { backups = []; backupHistoryLoaded = false; backupRunning = false; backupWaiting = false; }
    } finally { if (backupStatusController === controller) { backupStatusBusy = false; backupStatusController = undefined; } }
  }
  function backupElapsedLabel() { const seconds = backupElapsed % 60; const minutes = Math.floor(backupElapsed / 60); return minutes ? `${minutes}m ${seconds}s` : `${seconds}s`; }
  function fail(reason: unknown) { error = reason instanceof Error ? reason.message : 'The operation failed.'; notice = ''; }
  function success(value: string) { notice = value; error = ''; }
  async function saveIdentity(event: SubmitEvent) { event.preventDefault(); if (!config) return; if (!config.navigation.show_dashboard && !config.navigation.show_records) return fail(new Error('At least Dashboard or Records must remain visible.')); busy = 'deployment'; try { const result = await saveDeployment(config); configRevision = result.revision; success('Deployment identity saved. Reloaded clients receive it through live sync.'); } catch (reason) { fail(reason); } finally { busy = ''; } }
  async function uploadLogo() { if (!logoFile || !config) return; busy = 'logo'; logoProgress = 0; try { const result = await uploadDeploymentLogo(logoFile, (loaded,total) => logoProgress = total ? Math.round(loaded / total * 100) : 0); config = result.config; logoFile = null; success('Logo uploaded to N1 and published across MX.'); } catch (reason) { fail(reason); } finally { busy = ''; } }
  async function backup() {
    if (backupActive || !backupHistoryLoaded || busy) return;
    backupBaseline = new Set(backups.map(item => item.uid)); backupObservedRun = false; backupUnconfirmed = false; backupEmptyChecks = 0;
    backupPending = true; backupWaiting = false; backupStartedAt = Date.now(); backupElapsed = 0; backupOwnStart = true;
    backupMessage = ''; backupProblem = ''; backupConnection = '';
    ++backupStatusSequence;
    try {
      const result = await createBackup();
      if (!alive) return;
      ++backupStatusSequence; backupRunning = false; backupWaiting = false; backupObservedRun = false;
      backups = [result.backup, ...backups.filter(item => item.uid !== result.backup.uid)];
      backupMessage = `Backup saved to N1 and verified: ${result.backup.file_name}`;
    } catch (reason) {
      if (!alive) return;
      if (reason instanceof ConnectionError && reason.uncertainWrite || reason instanceof ApiError && reason.status === 502 && reason.payload === null) {
        backupWaiting = true; backupUnconfirmed = true;
        backupProblem = 'MX has not confirmed the result. Checking backup status automatically; do not start another backup yet.';
      } else if (reason instanceof ApiError && reason.status === 409) {
        backupRunning = true; backupObservedRun = true; backupOwnStart = false;
        backupProblem = '';
      } else { backupWaiting = true; backupProblem = reason instanceof Error ? reason.message : 'The backup could not be created.'; }
    } finally { if (alive) { backupPending = false; void refreshBackupStatus(); } }
  }
  async function verify(item: BackupEntry) { if (busy || backupActive) return; busy = item.uid; backupProblem = ''; backupMessage = ''; try { await verifyBackup(item.uid); if (alive) { backupMessage = `${item.file_name} passed verification.`; await refreshBackupStatus(); } } catch (reason) { if (alive) backupProblem = reason instanceof Error ? reason.message : 'Backup verification failed.'; } finally { if (alive) busy = ''; } }
  async function filterAudit(event?: SubmitEvent) { event?.preventDefault(); busy = 'audit'; try { audit = await loadAudit(auditPage, auditUser, auditAction, auditResult); } catch (reason) { fail(reason); } finally { busy = ''; } }
  async function pageAudit(page: number) { auditPage = page; await filterAudit(); }
  function bytes(value: number) { if (value < 1024) return `${value} B`; if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KB`; return `${(value / 1024 ** 2).toFixed(1)} MB`; }
  function when(value: number | null) { if (!value) return '—'; return new Date(value > 10_000_000_000 ? value : value * 1000).toLocaleString(); }
  function actionLabel(action: string) {
    const labels: Record<string, string> = {
      'record.create': 'Created record', 'record.update': 'Updated record', 'record.delete': 'Deleted record',
      'attachment.upload': 'Uploaded attachment', 'attachment.delete': 'Deleted attachment', 'attachment.preview': 'Previewed attachment', 'attachment.download': 'Downloaded attachment',
      'account.list': 'Viewed accounts', 'account.create': 'Created account', 'account.modify': 'Changed account', 'account.delete': 'Deleted account',
      'account.self.modify': 'Changed own account', 'account.self.delete': 'Deleted own account', 'account.self.profile': 'Updated own profile', 'account.self.password': 'Changed own password',
      'account.self.2fa.enroll': 'Changed own 2FA enrollment', 'account.self.2fa.disable': 'Disabled own 2FA', 'account.self.recovery-codes.regenerate': 'Regenerated recovery codes',
      'account.admin.password-reset': 'Reset account password', 'account.admin.security-reset': 'Reset account authenticator',
      'database.query': 'Ran database query', 'audit.view': 'Viewed audit log',
      'backup.create': 'Created database backup', 'backup.verify': 'Verified database backup', 'backup.download': 'Downloaded database backup',
      'module.delete': 'Deleted module',
      'schema.field.create': 'Created record field', 'schema.field.update': 'Changed record field', 'schema.order.update': 'Reordered record fields',
      'dashboard.config.update': 'Published dashboard', 'storage.layout.update': 'Changed N1 storage layout', 'deployment.config.update': 'Changed deployment identity'
    };
    return labels[action] || action.replace(/[._-]+/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
  }
  async function restoreDefaults() {
    if (!config || !await requestConfirmation({ title: 'Restore MX identity defaults?', description: 'Only this form will be reset. Nothing changes for users until you save the deployment identity.', confirmLabel: 'Restore defaults', tone: 'primary' })) return;
    config = {
      branding: { display_name: 'MX', subtitle: "Litiaina's Digital Workplace Platform", organization_name: '', logo_url: 'images/system-icon.png' },
      appearance: { preset: 'blue', primary_color: '#1d4ed8', sidebar_color: '#0f172a', radius: 'rounded', density: 'normal', default_theme: 'light', content_width: 'wide' },
      terminology: { record_singular: 'Record', record_plural: 'Records', dashboard_label: 'Dashboard', administration_label: 'Administration' },
      navigation: { show_dashboard: true, show_records: true, show_quick_actions: true, default_workspace: 'dashboard' },
      collaboration: { message_page_size: 256 }
    };
    success('Standard MX settings restored in the editor. Save identity to publish them.');
  }
  function applyPreset() {
    if (!config) return;
    const colors: Record<string, [string, string]> = { blue: ['#1d4ed8', '#0f172a'], emerald: ['#047857', '#064e3b'], violet: ['#6d28d9', '#2e1065'], amber: ['#b45309', '#451a03'], rose: ['#be123c', '#4c0519'], slate: ['#475569', '#0f172a'] };
    const selected = colors[config.appearance.preset];
    if (selected) [config.appearance.primary_color, config.appearance.sidebar_color] = selected;
  }
</script>

<div class={`operations-section operations-section-${section}`}>
{#if notice}<div class="notice success">{notice}</div>{/if}{#if error}<div class="notice error">{error}</div>{/if}
{#if config}
  <form class="panel deployment-form" onsubmit={saveIdentity}>
    <div class="panel-heading"><div><h2>Deployment identity</h2><p class="muted">Branding, appearance, terminology, and navigation · revision {configRevision}</p></div><div class="inline-actions"><button class="button" type="button" onclick={restoreDefaults}>Restore MX defaults</button><button class="button primary" disabled={busy === 'deployment'}>Save identity</button></div></div>
    <div class="settings-grid">
      <div class="settings-column">
        <fieldset><legend>Branding</legend><label>Display name<input bind:value={config.branding.display_name} /></label><label>Subtitle<input bind:value={config.branding.subtitle} /></label><label>Organization<input bind:value={config.branding.organization_name} /></label><label>Logo URL<input bind:value={config.branding.logo_url} /></label></fieldset>
        <fieldset><legend>Terminology</legend><label>Record singular<input bind:value={config.terminology.record_singular} /></label><label>Record plural<input bind:value={config.terminology.record_plural} /></label><label>Dashboard label<input bind:value={config.terminology.dashboard_label} /></label><label>Administration label<input bind:value={config.terminology.administration_label} /></label></fieldset>
        <fieldset><legend>Navigation</legend><label class="checkbox"><input type="checkbox" bind:checked={config.navigation.show_dashboard} /> Show dashboard</label><label class="checkbox"><input type="checkbox" bind:checked={config.navigation.show_records} /> Show records</label><label class="checkbox"><input type="checkbox" bind:checked={config.navigation.show_quick_actions} /> Show quick actions</label><label>Default workspace<select bind:value={config.navigation.default_workspace}><option value="dashboard">Dashboard</option><option value="records">Records</option></select></label></fieldset>
      </div>
      <div class="settings-column">
        <fieldset><legend>Appearance</legend><label>Preset<select bind:value={config.appearance.preset} onchange={applyPreset}><option value="blue">Blue</option><option value="emerald">Emerald</option><option value="violet">Violet</option><option value="amber">Amber</option><option value="rose">Rose</option><option value="slate">Slate</option><option value="custom">Custom</option></select></label><label>Primary color<input bind:value={config.appearance.primary_color} type="color" disabled={config.appearance.preset !== 'custom'} /></label><label>Sidebar color<input bind:value={config.appearance.sidebar_color} type="color" disabled={config.appearance.preset !== 'custom'} /></label><label>Corner style<select bind:value={config.appearance.radius}><option value="square">Square</option><option value="subtle">Subtle</option><option value="rounded">Rounded</option><option value="soft">Soft</option></select></label><label>Density<select bind:value={config.appearance.density}><option value="compact">Compact</option><option value="normal">Normal</option><option value="comfortable">Comfortable</option></select></label><label>Content width<select bind:value={config.appearance.content_width}><option value="standard">Standard</option><option value="wide">Wide</option><option value="full">Full</option></select></label><label>Default theme<select bind:value={config.appearance.default_theme}><option value="light">Light</option><option value="dark">Dark</option><option value="system">System</option></select></label></fieldset>
        <fieldset><legend>Collaboration</legend><label>Messages per page<input bind:value={config.collaboration.message_page_size} type="number" min="32" max="512" step="32" /></label><p class="muted">MX loads this many recent messages, then automatically fetches older pages as people scroll upward. Default: 256.</p></fieldset>
      </div>
    </div>
    <section class="identity-preview" style={`--preview-primary:${config.appearance.primary_color};--preview-sidebar:${config.appearance.sidebar_color};`}><aside><div class="preview-logo">{#if config.branding.logo_url}<img src={config.branding.logo_url} alt="" />{:else}{config.branding.display_name.slice(0, 2).toUpperCase()}{/if}</div><strong>{config.branding.display_name || 'MX'}</strong><small>{config.branding.subtitle || 'Information system'}</small>{#if config.branding.organization_name}<small>{config.branding.organization_name}</small>{/if}<nav>{#if config.navigation.show_dashboard}<span class="active"><LayoutDashboard size={13} />{config.terminology.dashboard_label || 'Dashboard'}</span>{/if}{#if config.navigation.show_records}<span><FileText size={13} />{config.terminology.record_plural || 'Records'}</span>{/if}<span><ShieldCheck size={13} />{config.terminology.administration_label || 'Administration'}</span></nav></aside><main><small>Live preview</small><h3>{config.terminology.dashboard_label || 'Dashboard'}</h3><div><i></i><i></i><i></i></div></main></section>
  </form>
{/if}

<section class="panel section-panel"><div class="panel-heading"><div><h2>System logo</h2><p class="muted">Upload PNG, JPEG, WebP, or GIF. MX stores it in N1 and uses the same asset on sign-in, navigation, and browser metadata.</p></div>{#if config?.branding.logo_url}<img class="branding-logo-preview" src={config.branding.logo_url} alt="Current system logo" />{/if}</div><div class="logo-upload-row"><label class="file-picker"><span class="file-picker-icon"><Upload size={19} /></span><span class="file-picker-copy"><strong>{logoFile?.name || 'Choose a logo image'}</strong><small>Maximum 5 MiB</small></span><input type="file" accept="image/png,image/jpeg,image/webp,image/gif" onchange={(event) => logoFile = event.currentTarget.files?.[0] || null} /></label><button class="button primary" onclick={uploadLogo} disabled={!logoFile || busy === 'logo'}>{busy === 'logo' ? `Uploading ${logoProgress}%…` : 'Upload to N1'}</button></div></section>

<section class="panel section-panel backup-panel" aria-label="Database backups">
  <div class="panel-heading"><div><h2>Database backups</h2><p class="muted">Encrypted transport to the configured N1 fragment with SQLite integrity verification.</p></div><div class="inline-actions backup-actions"><button class="button" type="button" onclick={() => void refreshBackupStatus()} disabled={backupStatusBusy}>Refresh status</button><button class="button primary" type="button" onclick={backup} disabled={backupActive || !backupHistoryLoaded || !!busy} aria-busy={backupActive}>{#if backupActive}<span class="backup-spinner" aria-hidden="true"><LoaderCircle size={16} /></span>{/if}{backupPending ? 'Creating backup…' : backupRunning ? 'Backup running…' : backupWaiting ? 'Checking backup…' : 'Create backup'}</button></div></div>
  {#if backupActive}<div class="backup-progress" role="status" aria-live="polite"><div class="backup-progress-heading"><strong>{backupWaiting && !backupPending && !backupRunning ? 'Waiting for MX to confirm the backup' : 'Database backup in progress'}</strong><span class="backup-elapsed" aria-live="off">{backupOwnStart ? 'Elapsed' : 'Observed for'} {backupElapsedLabel()}</span></div><progress aria-label="Database backup activity"></progress><p>MX creates a SQLite snapshot, checks its integrity and stores it in N1. Success appears here only after MX confirms completion.</p></div>{/if}
  {#if backupConnection}<div class="notice warning backup-feedback" role="alert"><strong>Backup status unavailable.</strong> {backupConnection}{#if backupActive} The last known activity is shown above; MX will check again automatically.{/if}</div>{/if}
  {#if backupProblem}<div class="notice error backup-feedback" role="alert">{backupProblem}</div>{/if}
  {#if backupMessage}<div class="notice success backup-feedback" role="status"><CircleCheck size={18} aria-hidden="true" /><span>{backupMessage}</span></div>{/if}
  <div class="table-wrap"><table><thead><tr><th>File</th><th>Size</th><th>Status</th><th>Created</th><th>Verified</th><th></th></tr></thead><tbody>{#if !backups.length}<tr><td colspan="6">{!backupHistoryLoaded ? backupConnection ? 'Backup history could not be loaded. Use Refresh status to try again.' : 'Loading backup history…' : 'No backup history yet.'}</td></tr>{/if}{#each backups as item}<tr><td><strong>{item.file_name}</strong><small class="block">{item.integrity_check}</small></td><td>{bytes(item.size)}</td><td><span class:success-text={item.status === 'verified'}>{item.status}</span></td><td>{when(item.created_at)}</td><td>{when(item.verified_at)}</td><td><div class="inline-actions"><button class="button small" type="button" onclick={() => verify(item)} disabled={!!busy || backupActive} aria-busy={busy === item.uid}>{#if busy === item.uid}<span class="backup-spinner" aria-hidden="true"><LoaderCircle size={14} /></span>Verifying…{:else}Verify{/if}</button><button class="button small" onclick={() => downloadBackup(item.uid)}>Download</button></div></td></tr>{/each}</tbody></table></div>
</section>

<section class="panel section-panel"><div class="panel-heading"><div><h2>Audit log</h2><p class="muted">{audit?.total || 0} security and data event{audit?.total === 1 ? '' : 's'}.</p></div></div><div class="audit-summary"><div><span>Loaded events</span><strong>{audit?.data.length || 0}</strong></div><div><span>Within 24 hours</span><strong>{auditDay}</strong></div><div><span>Failed in this page</span><strong>{auditFailed}</strong></div><div><span>Actors in this page</span><strong>{auditActors}</strong></div></div><form class="audit-filters" onsubmit={(event) => { auditPage = 1; void filterAudit(event); }}><input bind:value={auditUser} placeholder="User, email, or UID" /><input bind:value={auditAction} placeholder="Action, route, or target" /><select bind:value={auditResult}><option value="all">All results</option><option value="success">Successful</option><option value="failed">Failed</option></select><button class="button">Filter</button></form><div class="table-wrap"><table><thead><tr><th>Time</th><th>Actor</th><th>Action</th><th>Target</th><th>Result</th></tr></thead><tbody>{#if !(audit?.data.length)}<tr><td colspan="5">No audit events match these filters.</td></tr>{/if}{#each audit?.data || [] as row}<tr><td>{when(row.created_at)}</td><td><strong>{row.actor_name}</strong><small class="block">{row.actor_email}</small></td><td><strong>{actionLabel(row.action)}</strong><small class="block">{row.method} {row.path} · {row.action}</small></td><td>{row.target_uid || '—'}</td><td><span class:success-text={row.success} class:error-text={!row.success}>{row.status_code} · {row.success ? 'success' : 'failed'}</span></td></tr>{/each}</tbody></table></div><div class="pagination"><button class="button" disabled={(audit?.page || 1) <= 1} onclick={() => pageAudit((audit?.page || 1) - 1)}>Previous</button><span>Page {audit?.total_pages ? audit.page : 0} of {audit?.total_pages || 0}</span><button class="button" disabled={!audit?.has_next} onclick={() => pageAudit((audit?.page || 1) + 1)}>Next</button></div></section>
</div>

<style>
  .backup-panel>.panel-heading{flex-wrap:wrap;gap:.75rem;margin-bottom:.85rem;}
  .backup-panel>.panel-heading>div:first-child{flex:1;min-width:min(100%,22rem);}
  .backup-actions{flex-wrap:wrap;}
  .backup-spinner{display:inline-flex;flex-shrink:0;animation:backup-spin 1s linear infinite;}
  .backup-progress{display:grid;gap:.65rem;margin:.2rem 0 1rem;padding:.85rem 1rem;border:1px solid var(--line);border-radius:.5rem;background:var(--surface-2);font-size:calc(.8rem * var(--font-scale));}
  .backup-progress-heading{display:flex;align-items:center;justify-content:space-between;gap:.5rem;flex-wrap:wrap;}
  .backup-elapsed{color:var(--muted);font-variant-numeric:tabular-nums;font-size:calc(.75rem * var(--font-scale));}
  .backup-progress progress{display:block;width:100%;height:.45rem;accent-color:var(--primary);}
  .backup-progress p{margin:0;color:var(--muted);line-height:1.5;}
  .backup-feedback{display:flex;align-items:center;gap:.5rem;overflow-wrap:anywhere;}
  .backup-feedback>span{min-width:0;}
  @keyframes backup-spin{to{transform:rotate(360deg);}}
  @media(prefers-reduced-motion:reduce){.backup-spinner{animation:none;}}
  @media(max-width:600px){.backup-actions{width:100%;}.backup-actions .button{flex:1;}.backup-progress{padding:.75rem;}}
  /* Deployment only: independent stacks prevent Appearance's height from
     creating blank space between Branding and Terminology. Do not restyle Drive. */
  .deployment-form { container: deployment-settings / inline-size; }
  .settings-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); align-items: start; gap: 1rem; }
  .settings-column { min-width: 0; display: grid; align-content: start; gap: 1rem; }
  .settings-grid fieldset { min-width: 0; display: grid; align-content: start; gap: .75rem; }
  .settings-grid label { min-width: 0; align-self: start; font-size: calc(.75rem * var(--font-scale)); font-weight: 600; }
  .settings-grid input:not([type='checkbox']), .settings-grid select {
    width: 100%; min-width: 0;
    height: max(calc(2.375rem * var(--interface-factor)), calc(.8125rem * var(--font-scale) * 1.4 + 1rem * var(--interface-factor) + 2px));
    min-height: 0;
    padding: calc(.5rem * var(--interface-factor)) calc(.65rem * var(--interface-factor));
    font-size: calc(.8125rem * var(--font-scale)); font-weight: 400; line-height: 1.4;
  }
  .settings-grid input[type='color'] { padding: .25rem; }
  .settings-grid p { margin: 0; font-size: calc(.75rem * var(--font-scale)); }
  .deployment-form > .panel-heading { flex-wrap: wrap; }
  @container deployment-settings (max-width: 48rem) {
    .settings-grid { grid-template-columns: minmax(0, 1fr); }
  }
  @media (max-width: 600px) {
    .deployment-form > .panel-heading > .inline-actions { width: 100%; }
    .deployment-form > .panel-heading > .inline-actions .button { flex: 1 1 auto; }
  }
</style>
