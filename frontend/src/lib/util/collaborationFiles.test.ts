import { describe, expect, it } from 'vitest';
import { clipboardFiles, transferredFiles, transferHasFiles, uniqueFiles } from './collaborationFiles';

const file = (name: string, type = 'application/octet-stream') => new File(['content'], name, { type, lastModified: 1 });
const transfer = (files: File[], items: object[] = []) => ({ types: files.length ? ['Files'] : ['text/plain'], files, items }) as unknown as DataTransfer;
describe('Collaboration file input', () => {
  it('accepts non-image clipboard files and deduplicates the items/files representations', () => {
    const files = [file('Report.docx'), file('Budget.xlsx'), file('Notes.txt', 'text/plain')];
    const data = transfer(files, files.map(value => ({ kind: 'file', getAsFile: () => value })));
    expect(transferHasFiles(data)).toBe(true);
    expect(transferredFiles(data)).toEqual({ files, directories: false });
  });
  it('supports files-only clipboard data and protected dragover data', () => {
    expect(transferredFiles(transfer([file('Report.pdf')])).files).toHaveLength(1);
    expect(transferHasFiles({ types: ['Files'], files: [], items: [] } as unknown as DataTransfer)).toBe(true);
    expect(transferHasFiles(null)).toBe(false);
    expect(transferHasFiles(transfer([]))).toBe(false);
  });
  it('does not silently turn a directory into an empty attachment', () => {
    const directory = file('Folder'), actual = file('Actual.txt');
    const data = transfer([directory, actual], [{ kind: 'file', webkitGetAsEntry: () => ({ isDirectory: true, name: 'Folder' }), getAsFile: () => directory }]);
    expect(transferredFiles(data)).toEqual({ files: [actual], directories: true });
  });
  it('preserves document names and gives generic clipboard images a meaningful name', () => {
    const report = file('Report.xlsx'), image = file('image.png', 'image/png'), named = file('Photo.jpg', 'image/jpeg');
    const result = clipboardFiles([report, image, named], 'test-time');
    expect(result[0]).toBe(report); expect(result[1].name).toBe('Screenshot-test-time-2.png'); expect(result[2]).toBe(named);
  });
  it('deduplicates against both existing attachments and the current incoming batch', () => {
    const first = file('One.txt'), second = file('Two.txt');
    expect(uniqueFiles([first, second, second], [first])).toEqual([second]);
  });
});
