<script lang="ts">
  import { onMount } from 'svelte';
  import type { AuditPage, BackupEntry, DeploymentConfig } from '../../api/domain';
  import { createBackup, downloadBackup, listBackups, loadAudit, loadDeployment, saveDeployment, verifyBackup } from '../../api/workspace';

  let config = $state<DeploymentConfig | null>(null); let configRevision = $state(0);
  let backups = $state<BackupEntry[]>([]); let backupRunning = $state(false);
  let audit = $state<AuditPage | null>(null); let auditPage = $state(1); let auditUser = $state(''); let auditAction = $state(''); let auditResult = $state('all');
  let loading = $state(true); let busy = $state(''); let error = $state(''); let notice = $state('');
  const auditDay = $derived((audit?.data || []).filter((entry) => { const value = entry.created_at > 10_000_000_000 ? entry.created_at : entry.created_at * 1000; return value >= Date.now() - 86_400_000; }).length);
  const auditFailed = $derived((audit?.data || []).filter((entry) => !entry.success).length);
  const auditActors = $derived(new Set((audit?.data || []).map((entry) => entry.actor_uid).filter(Boolean)).size);

  onMount(() => void refresh());
  async function refresh() { loading = true; try { const [deployment, backupData, auditData] = await Promise.all([loadDeployment(), listBackups(), loadAudit(auditPage, auditUser, auditAction, auditResult)]); config = deployment.config; configRevision = deployment.revision; backups = backupData.backups; backupRunning = backupData.backup_running; audit = auditData; } catch (reason) { fail(reason); } finally { loading = false; } }
  function fail(reason: unknown) { error = reason instanceof Error ? reason.message : 'The operation failed.'; notice = ''; }
  function success(value: string) { notice = value; error = ''; }
  async function saveIdentity(event: SubmitEvent) { event.preventDefault(); if (!config) return; if (!config.navigation.show_dashboard && !config.navigation.show_records) return fail(new Error('At least Dashboard or Records must remain visible.')); busy = 'deployment'; try { const result = await saveDeployment(config); configRevision = result.revision; success('Deployment identity saved. Reloaded clients receive it through live sync.'); } catch (reason) { fail(reason); } finally { busy = ''; } }
  async function backup() { busy = 'backup'; try { await createBackup(); success('Database backup created and verified.'); await refresh(); } catch (reason) { fail(reason); } finally { busy = ''; } }
  async function verify(item: BackupEntry) { busy = item.uid; try { await verifyBackup(item.uid); success(`${item.file_name} passed verification.`); await refresh(); } catch (reason) { fail(reason); } finally { busy = ''; } }
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
      'schema.field.create': 'Created record field', 'schema.field.update': 'Changed record field', 'schema.order.update': 'Reordered record fields',
      'dashboard.config.update': 'Published dashboard', 'storage.layout.update': 'Changed N1 storage layout', 'deployment.config.update': 'Changed deployment identity'
    };
    return labels[action] || action.replace(/[._-]+/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
  }
  function restoreDefaults() {
    if (!config || !confirm('Restore the standard MX identity settings in this form? Nothing changes until you save.')) return;
    config = {
      branding: { display_name: 'MX', subtitle: "Litiaina's General-Purpose System", organization_name: '', logo_url: 'images/system-icon.png' },
      appearance: { preset: 'blue', primary_color: '#1d4ed8', sidebar_color: '#0f172a', radius: 'rounded', density: 'normal', default_theme: 'light', content_width: 'wide' },
      terminology: { record_singular: 'Record', record_plural: 'Records', dashboard_label: 'Dashboard', administration_label: 'Administration' },
      navigation: { show_dashboard: true, show_records: true, show_quick_actions: true, default_workspace: 'dashboard' }
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

{#if notice}<div class="notice success">{notice}</div>{/if}{#if error}<div class="notice error">{error}</div>{/if}
{#if config}
  <form class="panel deployment-form" onsubmit={saveIdentity}>
    <div class="panel-heading"><div><h2>Deployment identity</h2><p class="muted">Branding, appearance, terminology, and navigation · revision {configRevision}</p></div><div class="inline-actions"><button class="button" type="button" onclick={restoreDefaults}>Restore MX defaults</button><button class="button primary" disabled={busy === 'deployment'}>Save identity</button></div></div>
    <div class="settings-grid"><fieldset><legend>Branding</legend><label>Display name<input bind:value={config.branding.display_name} /></label><label>Subtitle<input bind:value={config.branding.subtitle} /></label><label>Organization<input bind:value={config.branding.organization_name} /></label><label>Logo URL<input bind:value={config.branding.logo_url} /></label></fieldset>
      <fieldset><legend>Appearance</legend><label>Preset<select bind:value={config.appearance.preset} onchange={applyPreset}><option value="blue">Blue</option><option value="emerald">Emerald</option><option value="violet">Violet</option><option value="amber">Amber</option><option value="rose">Rose</option><option value="slate">Slate</option><option value="custom">Custom</option></select></label><label>Primary color<input bind:value={config.appearance.primary_color} type="color" disabled={config.appearance.preset !== 'custom'} /></label><label>Sidebar color<input bind:value={config.appearance.sidebar_color} type="color" disabled={config.appearance.preset !== 'custom'} /></label><label>Corner style<select bind:value={config.appearance.radius}><option value="square">Square</option><option value="subtle">Subtle</option><option value="rounded">Rounded</option><option value="soft">Soft</option></select></label><label>Density<select bind:value={config.appearance.density}><option value="compact">Compact</option><option value="normal">Normal</option><option value="comfortable">Comfortable</option></select></label><label>Content width<select bind:value={config.appearance.content_width}><option value="standard">Standard</option><option value="wide">Wide</option><option value="full">Full</option></select></label><label>Default theme<select bind:value={config.appearance.default_theme}><option value="light">Light</option><option value="dark">Dark</option><option value="system">System</option></select></label></fieldset>
      <fieldset><legend>Terminology</legend><label>Record singular<input bind:value={config.terminology.record_singular} /></label><label>Record plural<input bind:value={config.terminology.record_plural} /></label><label>Dashboard label<input bind:value={config.terminology.dashboard_label} /></label><label>Administration label<input bind:value={config.terminology.administration_label} /></label></fieldset>
      <fieldset><legend>Navigation</legend><label class="checkbox"><input type="checkbox" bind:checked={config.navigation.show_dashboard} /> Show dashboard</label><label class="checkbox"><input type="checkbox" bind:checked={config.navigation.show_records} /> Show records</label><label class="checkbox"><input type="checkbox" bind:checked={config.navigation.show_quick_actions} /> Show quick actions</label><label>Default workspace<select bind:value={config.navigation.default_workspace}><option value="dashboard">Dashboard</option><option value="records">Records</option></select></label></fieldset></div>
    <section class="identity-preview" style={`--preview-primary:${config.appearance.primary_color};--preview-sidebar:${config.appearance.sidebar_color};`}><aside><div class="preview-logo">{#if config.branding.logo_url}<img src={config.branding.logo_url} alt="" />{:else}{config.branding.display_name.slice(0, 2).toUpperCase()}{/if}</div><strong>{config.branding.display_name || 'MX'}</strong><small>{config.branding.subtitle || 'Information system'}</small>{#if config.branding.organization_name}<small>{config.branding.organization_name}</small>{/if}<nav>{#if config.navigation.show_dashboard}<span class="active">⌂ {config.terminology.dashboard_label || 'Dashboard'}</span>{/if}{#if config.navigation.show_records}<span>▤ {config.terminology.record_plural || 'Records'}</span>{/if}<span>⚙ {config.terminology.administration_label || 'Administration'}</span></nav></aside><main><small>Live preview</small><h3>{config.terminology.dashboard_label || 'Dashboard'}</h3><div><i></i><i></i><i></i></div></main></section>
  </form>
{/if}

<section class="panel section-panel"><div class="panel-heading"><div><h2>Database backups</h2><p class="muted">Encrypted transport to the configured N1 fragment with SQLite integrity verification.</p></div><button class="button primary" onclick={backup} disabled={backupRunning || busy === 'backup'}>{backupRunning ? 'Backup running…' : 'Create backup'}</button></div><div class="table-wrap"><table><thead><tr><th>File</th><th>Size</th><th>Status</th><th>Created</th><th>Verified</th><th></th></tr></thead><tbody>{#if !backups.length}<tr><td colspan="6">No backup history yet.</td></tr>{/if}{#each backups as item}<tr><td><strong>{item.file_name}</strong><small class="block">{item.integrity_check}</small></td><td>{bytes(item.size)}</td><td><span class:success-text={item.status === 'verified'}>{item.status}</span></td><td>{when(item.created_at)}</td><td>{when(item.verified_at)}</td><td><div class="inline-actions"><button class="button small" onclick={() => verify(item)} disabled={busy === item.uid}>Verify</button><button class="button small" onclick={() => downloadBackup(item.uid)}>Download</button></div></td></tr>{/each}</tbody></table></div></section>

<section class="panel section-panel"><div class="panel-heading"><div><h2>Audit log</h2><p class="muted">{audit?.total || 0} security and data event{audit?.total === 1 ? '' : 's'}.</p></div></div><div class="audit-summary"><div><span>Loaded events</span><strong>{audit?.data.length || 0}</strong></div><div><span>Within 24 hours</span><strong>{auditDay}</strong></div><div><span>Failed in this page</span><strong>{auditFailed}</strong></div><div><span>Actors in this page</span><strong>{auditActors}</strong></div></div><form class="audit-filters" onsubmit={(event) => { auditPage = 1; void filterAudit(event); }}><input bind:value={auditUser} placeholder="User, email, or UID" /><input bind:value={auditAction} placeholder="Action, route, or target" /><select bind:value={auditResult}><option value="all">All results</option><option value="success">Successful</option><option value="failed">Failed</option></select><button class="button">Filter</button></form><div class="table-wrap"><table><thead><tr><th>Time</th><th>Actor</th><th>Action</th><th>Target</th><th>Result</th></tr></thead><tbody>{#if !(audit?.data.length)}<tr><td colspan="5">No audit events match these filters.</td></tr>{/if}{#each audit?.data || [] as row}<tr><td>{when(row.created_at)}</td><td><strong>{row.actor_name}</strong><small class="block">{row.actor_email}</small></td><td><strong>{actionLabel(row.action)}</strong><small class="block">{row.method} {row.path} · {row.action}</small></td><td>{row.target_uid || '—'}</td><td><span class:success-text={row.success} class:error-text={!row.success}>{row.status_code} · {row.success ? 'success' : 'failed'}</span></td></tr>{/each}</tbody></table></div><div class="pagination"><button class="button" disabled={(audit?.page || 1) <= 1} onclick={() => pageAudit((audit?.page || 1) - 1)}>Previous</button><span>Page {audit?.total_pages ? audit.page : 0} of {audit?.total_pages || 0}</span><button class="button" disabled={!audit?.has_next} onclick={() => pageAudit((audit?.page || 1) + 1)}>Next</button></div></section>
