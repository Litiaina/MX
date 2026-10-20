import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const records = readFileSync(new URL('./components/RecordsView.svelte', import.meta.url), 'utf8');
const editor = readFileSync(new URL('./components/RecordEditor.svelte', import.meta.url), 'utf8');
const api = readFileSync(new URL('./api/workspace.ts', import.meta.url), 'utf8');
const workspace = readFileSync(new URL('./components/Workspace.svelte', import.meta.url), 'utf8');

describe('stateless record editing', () => {
  it('keeps table values read-only and edits only through the full record form', () => {
    expect(records).not.toContain('beginCellEdit');
    expect(records).not.toContain('cell-edit-button');
    expect(records).not.toContain('data-active-cell-editor');
    expect(records).not.toContain('patchRecord(');
    expect(records).not.toContain('claimRecordEdit');
    expect(records).not.toContain('maintainEditLease');
    expect(records).toContain('window.getSelection()?.toString()');
  });

  it('provides a visible row-level edit action without opening view mode first', () => {
    expect(records).toContain('class="button record-primary-action"');
    expect(records).toContain("openRecord(row, false, canUpdate)");
    expect(records).toContain("<span>Edit</span>");
    expect(records).toContain('<th class="record-actions-column">Actions</th>');
  });

  it('submits only changed fields and supports field-level conflict decisions', () => {
    expect(editor).toContain('patchRecord(current.uid, changes, workingRevision');
    expect(editor).toContain('function useLatest(conflict: FieldConflict)');
    expect(editor).toContain('function keepMine(conflict: FieldConflict)');
    expect(editor).not.toContain('claimRecordEdit');
    expect(editor).toContain('base_revision: workingRevision');
    expect(editor).toContain('original: cloneJson(original)');
  });

  it('uses one scalar base revision and has no editing-session network API', () => {
    expect(api).toContain("jsonRequest('PATCH', { changes, base_revision })");
    expect(api).not.toContain('recordEditingPath');
    expect(workspace).not.toContain('record.cell.editing');
  });
});
