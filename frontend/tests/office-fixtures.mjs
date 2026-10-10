// Small standards-based fixtures and ZIP inspection; no host Office dependency.
import { inflateRawSync } from 'node:zlib';
import {readFileSync} from 'node:fs';
function crc32(bytes) { let crc=0xffffffff; for(const byte of bytes){crc^=byte;for(let i=0;i<8;i++)crc=(crc>>>1)^((crc&1)?0xedb88320:0);}return(crc^0xffffffff)>>>0; }
export function zip(entries) {
  const locals=[],central=[];let offset=0;
  for(const [path,text] of Object.entries(entries)){
    const name=Buffer.from(path),data=Buffer.from(text),crc=crc32(data);
    const local=Buffer.alloc(30);local.writeUInt32LE(0x04034b50);local.writeUInt16LE(20,4);local.writeUInt32LE(crc,14);local.writeUInt32LE(data.length,18);local.writeUInt32LE(data.length,22);local.writeUInt16LE(name.length,26);
    locals.push(local,name,data);
    const c=Buffer.alloc(46);c.writeUInt32LE(0x02014b50);c.writeUInt16LE(20,4);c.writeUInt16LE(20,6);c.writeUInt32LE(crc,16);c.writeUInt32LE(data.length,20);c.writeUInt32LE(data.length,24);c.writeUInt16LE(name.length,28);c.writeUInt32LE(offset,42);central.push(c,name);offset+=local.length+name.length+data.length;
  }
  const directory=Buffer.concat(central),end=Buffer.alloc(22);end.writeUInt32LE(0x06054b50);end.writeUInt16LE(Object.keys(entries).length,8);end.writeUInt16LE(Object.keys(entries).length,10);end.writeUInt32LE(directory.length,12);end.writeUInt32LE(offset,16);return Buffer.concat([...locals,directory,end]);
}
export function unzip(bytes){
  let end=bytes.length-22;while(end>=0&&bytes.readUInt32LE(end)!==0x06054b50)end--;if(end<0)throw new Error('Not a ZIP Office document');
  let at=bytes.readUInt32LE(end+16);const result={};for(let i=0;i<bytes.readUInt16LE(end+10);i++){
    const method=bytes.readUInt16LE(at+10),size=bytes.readUInt32LE(at+20),n=bytes.readUInt16LE(at+28),e=bytes.readUInt16LE(at+30),c=bytes.readUInt16LE(at+32),local=bytes.readUInt32LE(at+42);
    const path=bytes.subarray(at+46,at+46+n).toString(),start=local+30+bytes.readUInt16LE(local+26)+bytes.readUInt16LE(local+28),data=bytes.subarray(start,start+size);
    result[path]=(method===8?inflateRawSync(data):data).toString();at+=46+n+e+c;
  }return result;
}
const head='<?xml version="1.0" encoding="UTF-8" standalone="yes"?>';
const types=parts=>head+`<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>${Object.entries(parts).map(([name,type])=>`<Override PartName="/${name}" ContentType="${type}"/>`).join('')}</Types>`;
const rels=entries=>head+`<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">${entries.map(([id,type,target])=>`<Relationship Id="${id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/${type}" Target="${target}"/>`).join('')}</Relationships>`;
export function officeFixture(extension) {
  // Our one-slide fixture normalized through isolated LibreOffice, including
  // the OOXML master/layout/theme relationships absent from the minimal sample.
  // Authoring is not a dependency of the test runner or deployed MX editor.
  if(extension==='.pptx')return Buffer.from(readFileSync(new URL('./fixtures/presentation.base64',import.meta.url),'utf8').trim(),'base64');
  if(extension==='.docx')return zip({
    '[Content_Types].xml':types({'word/document.xml':'application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml'}),
    '_rels/.rels':rels([['rId1','officeDocument','word/document.xml']]),
    'word/document.xml':head+'<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:rPr><w:b/><w:sz w:val="28"/></w:rPr><w:t>MX original document</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>'
  });
  if(extension==='.xlsx')return zip({
    '[Content_Types].xml':types({'xl/workbook.xml':'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml','xl/worksheets/sheet1.xml':'application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml'}),
    '_rels/.rels':rels([['rId1','officeDocument','xl/workbook.xml']]),
    'xl/workbook.xml':head+'<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Budget" sheetId="1" r:id="rId1"/></sheets></workbook>',
    'xl/_rels/workbook.xml.rels':rels([['rId1','worksheet','worksheets/sheet1.xml']]),
    'xl/worksheets/sheet1.xml':head+'<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>MX original sheet</t></is></c></row><row r="2"><c r="A2"><v>1000</v></c><c r="B2"><v>170</v></c><c r="C2"><f>A2*B2</f><v>170000</v></c></row></sheetData></worksheet>'
  });
  if(extension==='.pptx-minimal')return zip({
    '[Content_Types].xml':types({'ppt/presentation.xml':'application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml','ppt/slides/slide1.xml':'application/vnd.openxmlformats-officedocument.presentationml.slide+xml'}),
    '_rels/.rels':rels([['rId1','officeDocument','ppt/presentation.xml']]),
    'ppt/presentation.xml':head+'<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>',
    'ppt/_rels/presentation.xml.rels':rels([['rId1','slide','slides/slide1.xml']]),
    'ppt/slides/slide1.xml':head+'<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="914400" y="914400"/><a:ext cx="7315200" cy="1828800"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="2800"/><a:t>MX original slide</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>'
  });
  throw new Error(`No fixture for ${extension}`);
}
