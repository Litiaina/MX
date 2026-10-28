// Rehearse migrations on a SQLite backup. Never launch MX against the source DB.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {createHash} from 'node:crypto';
import {readFile,writeFile} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {startIsolatedMx} from './isolated-mx.mjs';
import {onTestDisk} from './concurrency-fixture.mjs';
assert.ok(onTestDisk(process.env.MX_TEST_ARTIFACTS)||process.env.MX_TEST_ARTIFACTS?.startsWith('/home/altear/Temp Environment/')||process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/ST1000LM049/'));
assert.ok(!process.env.MX_TEST_N1_URL);
assert.ok(process.env.MX_TEST_UPGRADE_DB&&process.env.MX_TEST_UPGRADE_ADMIN,'Set explicit read-only source database and credential JSON paths');
const tables=['users','mx_modules','mx_fields','mx_records','mx_record_values','mx_record_links','mx_record_versions','mx_record_version_values','mx_attachments','mx_record_version_attachments','mx_drive_items','mx_drive_versions','mx_drive_grants','mx_drive_space_grants','mx_messages','mx_message_record_links','mx_user_preferences'];
function fingerprint(path){const db=new DatabaseSync(path,{readOnly:true});db.exec('PRAGMA query_only=ON;BEGIN;');try{const result={};for(const name of tables){const statement=db.prepare(`SELECT * FROM "${name}"`);statement.setReadBigInts(true);const rows=statement.all();const hashes=rows.map(row=>createHash('sha256').update(JSON.stringify(row,(_,v)=>typeof v==='bigint'?v.toString():v instanceof Uint8Array?Buffer.from(v).toString('base64'):v)).digest('hex')).sort();result[name]={rows:rows.length,sha256:createHash('sha256').update(hashes.join('\n')).digest('hex')};}return result;}finally{db.exec('ROLLBACK;');db.close();}}
let before;const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)),{seedDatabase:resolve(process.env.MX_TEST_UPGRADE_DB),beforeStart:path=>{before=fingerprint(path);}});
const result={passed:false,checks:[],recordTotals:[],originalDatabaseOpenedReadOnly:true};
try{
 console.log(`Existing-data migration artifacts: ${mx.directory}`);
 assert.notEqual(resolve(process.env.MX_TEST_UPGRADE_DB),join(mx.directory,'test.db'));
 const credentials=JSON.parse(await readFile(process.env.MX_TEST_UPGRADE_ADMIN,'utf8'));
 const login=await mx.call('/mx/v1/auth/authenticate',null,'POST',{email:credentials.email,password:credentials.password},[200]);const token=login.body.access_token;
 const {body:{modules}}=await mx.call('/mx/v1/modules',token);
 for(const module of modules.filter(m=>m.active)){const {body:page}=await mx.call(`/mx/v1/modules/${module.uid}/records?limit=25`,token);assert.ok(page.data.length<=25);result.recordTotals.push(page.total);if(page.data[0])await mx.call(`/mx/v1/modules/${module.uid}/records/${page.data[0].uid}`,token);}
 assert.ok(result.recordTotals.reduce((a,b)=>a+b,0)>=10000);
 await mx.restart();
 const db=new DatabaseSync(join(mx.directory,'test.db'),{readOnly:true});result.ownerRows=db.prepare('SELECT COUNT(*) AS n FROM mx_record_owners').get().n;assert.ok(result.ownerRows>0);assert.equal(db.prepare('PRAGMA integrity_check').get().integrity_check,'ok');assert.deepEqual(db.prepare('PRAGMA foreign_key_check').all(),[]);db.close();
 assert.deepEqual(fingerprint(join(mx.directory,'test.db')),before);
 result.checks.push('Existing administrator login, module lists and records load after migration and restart','All 17 business-table row counts and complete content hashes unchanged','Creator provenance backfilled; SQLite integrity and foreign keys pass');result.passed=true;
 console.log(JSON.stringify({passed:true,records:before.mx_records.rows,modules:before.mx_modules.rows,driveItems:before.mx_drive_items.rows,ownerRows:result.ownerRows,unchangedBusinessTables:tables.length}));
}finally{await writeFile(join(mx.directory,'existing-data-migration-results.json'),JSON.stringify(result,null,2),{mode:0o600});await mx.close();}
