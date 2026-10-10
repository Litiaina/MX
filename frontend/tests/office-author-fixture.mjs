// One-time fixture authoring only; never used by MX or its normal test runner.
// Convert our synthetic OOXML through an isolated host LibreOffice profile to
// verify whether presentation input failures come from missing fixture layout.
import {mkdtemp,mkdir,writeFile,readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {execFileSync} from 'node:child_process';
import {officeFixture} from './office-fixtures.mjs';
const directory=await mkdtemp(join(tmpdir(),'mx-office-fixture-'));
await mkdir(join(directory,'source'));await mkdir(join(directory,'output'));
await writeFile(join(directory,'source','test.pptx'),officeFixture('.pptx-minimal'));
execFileSync('/usr/bin/libreoffice',[`-env:UserInstallation=file://${directory}/profile`,'--headless','--convert-to','pptx','--outdir',join(directory,'output'),join(directory,'source','test.pptx')],{stdio:'ignore',timeout:30000});
console.log(directory);console.log((await readFile(join(directory,'output','test.pptx'))).toString('base64'));
