// Explicit filters preserve the source format; never silently save XLSX as ODS.
export const officeFormats: Record<string, { filter: string; mime: string }> = {
  '.docx': { filter: 'Office Open XML Text', mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' },
  '.doc': { filter: 'MS Word 97', mime: 'application/msword' },
  '.odt': { filter: 'writer8', mime: 'application/vnd.oasis.opendocument.text' },
  '.rtf': { filter: 'Rich Text Format', mime: 'application/rtf' },
  '.xlsx': { filter: 'Calc MS Excel 2007 XML', mime: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' },
  '.xls': { filter: 'MS Excel 97', mime: 'application/vnd.ms-excel' },
  '.ods': { filter: 'calc8', mime: 'application/vnd.oasis.opendocument.spreadsheet' },
  '.pptx': { filter: 'Impress MS PowerPoint 2007 XML', mime: 'application/vnd.openxmlformats-officedocument.presentationml.presentation' },
  '.ppt': { filter: 'MS PowerPoint 97', mime: 'application/vnd.ms-powerpoint' },
  '.odp': { filter: 'impress8', mime: 'application/vnd.oasis.opendocument.presentation' }
};
export function officeExtension(name: string): string {
  const extension = name.match(/\.[^.\/\\]+$/)?.[0].toLowerCase() || '';
  return Object.hasOwn(officeFormats, extension) ? extension : '';
}
// Impress renders, but native editing has not passed the bundled-engine input
// tests. Keep this explicit gate until real edit/export round trips pass.
export const officePresentation = (extension: string) => ['.ppt', '.pptx', '.odp'].includes(extension);
