/** Materialize a recovered IndexedDB File before handing it to XHR. In
 * particular, Firefox can otherwise leave an IDB-backed upload body pending
 * after its originating editor has closed. Normal saves take no extra copy.
 * Keep every identity field: retries must retain the same upload operation. */
export async function recoveredUpload(file?: File, preparedBlob?:Blob): Promise<File|undefined> {
  if(!file)return;
  if(preparedBlob){if(preparedBlob.size!==file.size)throw new Error('The saved recovery payload is incomplete. No upload has started.');return new File([preparedBlob],file.name,{type:file.type,lastModified:file.lastModified});}
  const bytes=await file.arrayBuffer();
  if(bytes.byteLength!==file.size)throw new Error('The saved recovery payload is incomplete. Keep the local copy and download it; no upload has started.');
  return new File([bytes],file.name,{type:file.type,lastModified:file.lastModified});
}
