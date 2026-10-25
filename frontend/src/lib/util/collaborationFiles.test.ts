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
  it('queues one image when clipboard items/files create wrappers with different timestamps', () => {
    const itemFile = new File(['image bytes'], 'image.png', { type: 'image/png', lastModified: 1 });
    const listFile = new File(['image bytes'], 'image.png', { type: 'image/png', lastModified: 2 });
    const data = transfer([listFile], [{ kind: 'file', getAsFile: () => itemFile }]);
    const extracted = transferredFiles(data);
    expect(extracted.files).toEqual([itemFile]);
    expect(clipboardFiles(extracted.files, 'one-paste')).toHaveLength(1);
  });
  it('matches clipboard representations one-to-one without discarding additional files', () => {
    const first = new File(['first'], 'image.png', { type: 'image/png', lastModified: 1 });
    const second = new File(['other'], 'image.png', { type: 'image/png', lastModified: 2 });
    const wrapper = new File(['first'], 'image.png', { type: 'image/png', lastModified: 3 });
    const extra = file('Report.pdf', 'application/pdf');
    const data = transfer([wrapper, second, extra], [{ kind: 'file', getAsFile: () => first }]);
    expect(transferredFiles(data).files).toEqual([first, second, extra]);
    const complete = transfer([wrapper, new File(['other'], 'image.png', { type: 'image/png', lastModified: 4 })], [first, second].map(value => ({ kind: 'file', getAsFile: () => value })));
    expect(transferredFiles(complete).files).toEqual([first, second]);
  });
  it('keeps different selections distinct instead of ignoring their timestamps globally', () => {
    const first = new File(['first'], 'same.txt', { type: 'text/plain', lastModified: 1 });
    const changed = new File(['other'], 'same.txt', { type: 'text/plain', lastModified: 2 });
    expect(uniqueFiles([changed], [first])).toEqual([changed]);
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
