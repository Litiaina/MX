// Large read fixtures, never a deployment. This measures bounded read paths,
// not a certified sustained posting rate or a live N1/storage benchmark.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {DatabaseSync} from 'node:sqlite';
import {startIsolatedMx} from './isolated-mx.mjs';
assert.ok(process.env.MX_TEST_ARTIFACTS?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(process.env.TMPDIR?.startsWith('/mnt/PROJECT-DISK-1TB/'));
assert.ok(!process.env.MX_TEST_N1_URL);
const mx=await startIsolatedMx(fileURLToPath(new URL('../',import.meta.url)));let writer;const results={passed:false};
try{
  console.log(`Module books scale artifacts: ${mx.directory}`);const password=`Disposable!${randomUUID()}`;
  await mx.call('/mx/v1/auth/create',mx.signupKey,'POST',{email:'owner@book-scale.test',name:'Owner',password},[201]);const owner=(await mx.call('/mx/v1/auth/authenticate',null,'POST',{email:'owner@book-scale.test',password})).body;
  const call=async(path,method='GET',data)=>(await mx.call(path,owner.access_token,method,data)).body;
  const m=(await mx.call('/mx/v1/admin/modules',owner.access_token,'POST',{name:'Large operational book',singular_name:'Entry'},[201])).body.module;
  const base=`/mx/v1/modules/${m.uid}/books`,initial=await call(base);
  await call(base,'PUT',{revision:0,settings:{...initial.settings,accounting:true,inventory:true,currency:'PHP'}});
  const command=async data=>call(`${base}/commands`,'POST',{configuration_revision:1,operation_uid:randomUUID(),...data});
  const a=(await command({kind:'account',code:'CASH',name:'Cash',category:'asset'})).uid,b=(await command({kind:'account',code:'CAPITAL',name:'Capital',category:'equity'})).uid,item=(await command({kind:'item',code:'PAPER',name:'Paper',unit:'ream'})).uid;
  writer=new DatabaseSync(join(mx.directory,'test.db'));writer.exec('PRAGMA busy_timeout=1000;BEGIN IMMEDIATE');
  writer.prepare("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000) INSERT INTO mx_book_journals(uid,module_uid,date,description,actor_uid,created_at) SELECT 'scale-j-'||x,?,'2026-10-25','Synthetic read fixture',?,0 FROM n").run(m.uid,owner.uid);
  writer.prepare("INSERT INTO mx_book_lines SELECT uid,0,?,'CASH','Cash',100,0 FROM mx_book_journals WHERE module_uid=?").run(a,m.uid);
  writer.prepare("INSERT INTO mx_book_lines SELECT uid,1,?,'CAPITAL','Capital',0,100 FROM mx_book_journals WHERE module_uid=?").run(b,m.uid);
  writer.prepare('UPDATE mx_book_accounts SET debit=10000000 WHERE uid=?').run(a);writer.prepare('UPDATE mx_book_accounts SET credit=10000000 WHERE uid=?').run(b);
  writer.prepare("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000) INSERT INTO mx_book_stock_events(uid,module_uid,item_uid,movement,quantity,on_hand,reserved,note,actor_uid,created_at) SELECT 'scale-s-'||x,?,?,'receive',1000,x*1000,0,'Synthetic read fixture',?,0 FROM n").run(m.uid,item,owner.uid);
  writer.prepare('UPDATE mx_book_items SET on_hand=100000000 WHERE uid=?').run(item);writer.exec('COMMIT');
  writer.exec('BEGIN IMMEDIATE');let pages;const started=performance.now();try{pages=await Promise.all(Array.from({length:8},(_,i)=>call(`${base}/list/${i%2?'journals':'stock-events'}`)));await Promise.all([call(base),call(`${base}/list/accounts`),call(`${base}/list/items`),call(`${base}/journals/scale-j-1`)]);}finally{writer.exec('ROLLBACK');}
  results.held_writer_read_milliseconds=Math.round(performance.now()-started);assert.ok(results.held_writer_read_milliseconds<5000,'Reads do not wait on WAL writer');assert.ok(pages.every(v=>v.items.length===40&&v.has_next));
  const next=await call(`${base}/list/journals?after=${pages[1].next_cursor}`);assert.ok(next.items.every(v=>!pages[1].items.some(old=>old.uid===v.uid)));
  const balances=await call(`${base}/list/accounts`);assert.equal(balances.items[0].balance,'100000.00');assert.equal(balances.items[1].balance,'-100000.00');
  await command({kind:'stock',item_uid:item,movement:'reserve',quantity:'1.25',note:'Post after large history',source:null});assert.equal((await call(`${base}/list/items`)).items[0].available,'99998.750');
  await mx.restart();assert.equal((await call(`${base}/list/items`)).items[0].reserved,'1.250');assert.equal(writer.prepare('PRAGMA integrity_check').get().integrity_check,'ok');assert.equal(writer.prepare('PRAGMA foreign_key_check').all().length,0);
  results.journals=100000;results.journal_lines=200000;results.stock_events=100001;results.concurrent_readers=8;results.page_size=40;results.passed=true;console.log(JSON.stringify(results));
}finally{writer?.close();await writeFile(join(mx.directory,'module-books-scale-results.json'),JSON.stringify(results,null,2));await mx.close();}
