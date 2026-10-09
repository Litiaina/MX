import assert from 'node:assert/strict';
import { join } from 'node:path';

export async function verifyFileManager({page,engine,call,base,user,directory}) {
  const until=async(check,label)=>{const deadline=Date.now()+30000;while(Date.now()<deadline){if(await check())return;await new Promise(r=>setTimeout(r,100));}throw new Error(`File manager: ${label}`);};
  const name=(text)=>page.locator('.file-name').filter({hasText:text});
  const rows=page.locator('.file-row');
  await name(`notes-${engine}.txt`).click();
  assert.equal(await page.locator('.file-row.chosen').count(),1,'Single click selects, does not open');
  assert.equal(await page.locator('.file-preview-backdrop').count(),0);
  await name(`preview-${engine}.png`).click({modifiers:['Control']});
  assert.equal(await page.locator('.file-row.chosen').count(),2,'Ctrl click adds selection');
  await page.getByRole('button',{name:'Clear selection',exact:true}).click();await page.getByRole('checkbox',{name:`Select notes-${engine}.txt`,exact:true}).click();await page.keyboard.press('Control+c');await until(()=>page.getByRole('button',{name:'Paste 1 to copy',exact:true}).isVisible(),'Checkbox focus supports Ctrl+C');await name(`preview-${engine}.png`).click({modifiers:['Control']});
  await name(`notes-${engine}.txt`).click({button:'right'});
  assert.equal(await page.locator('.file-row.chosen').count(),2,'Right click preserves an existing multi-selection');
  const menu=page.getByRole('menu',{name:'Drive actions',exact:true}); await menu.waitFor();
  await page.keyboard.press('ArrowDown');assert.ok(await menu.evaluate(node=>node.contains(document.activeElement)),'Menu is keyboard navigable');
  await page.keyboard.press('Escape');await menu.waitFor({state:'hidden'});
  await rows.first().locator('.file-name').click();await rows.nth(2).locator('.file-name').click({modifiers:['Shift']});
  assert.equal(await page.locator('.file-row.chosen').count(),3,'Shift click selects a contiguous range');
  await page.getByRole('button',{name:'Clear selection',exact:true}).click();
  await page.locator('.selection-space').scrollIntoViewIfNeeded();
  const last=await rows.last().boundingBox(); const space=await page.locator('.selection-space').boundingBox();
  await page.mouse.move(space.x+5,space.y+8);await page.mouse.down();await page.mouse.move(last.x+last.width-8,last.y+4,{steps:12});
  assert.equal(await page.locator('.selection-marquee').count(),1,'Rubber band is visible during drag');
  await page.mouse.up();assert.ok(await page.locator('.file-row.chosen').count()>=1,'Rubber band selected intersecting rows');assert.equal(await page.locator('.selection-marquee').count(),0);
  await name(`notes-${engine}.txt`).click();await name(`preview-${engine}.png`).click({modifiers:['Control']});
  await page.keyboard.press('Control+c');await until(()=>page.getByRole('button',{name:'Paste 2 to copy',exact:true}).isVisible(),'Ctrl+C staged both files');
  await name(`UI ${engine}`).dblclick();await until(async()=>(await page.locator('.breadcrumbs').textContent()).includes(`UI ${engine}`),'destination folder opened');
  await page.keyboard.press('Control+v');await until(async()=>await rows.count()===2,'Ctrl+V copied two files');
  await name(`preview-${engine}.png`).dblclick();await page.waitForFunction(()=>[...document.querySelectorAll('.image-preview img')].some(image=>image.naturalWidth>0));await page.getByRole('button',{name:'Close preview',exact:true}).click();
  await page.keyboard.press('Control+v');await until(async()=>await rows.count()===4,'Repeated paste creates independent copies with distinct names');
  await name(`preview-${engine}.png`).click();await page.getByRole('button',{name:'Copy',exact:true}).click();await page.keyboard.press('Control+v');await until(async()=>await rows.count()===5,'Toolbar Copy supports keyboard Paste');
  const folder=(await call(base,user)).items.find(i=>i.name===`UI ${engine}`);
  const copied=(await call(`${base}?parent_uid=${folder.uid}`,user)).items;
  assert.ok(copied.some(i=>i.name===`notes-${engine} (copy).txt`),'Copy collision keeps original extension');
  await name(`notes-${engine}.txt`).click();await page.keyboard.press('Control+x');
  await until(()=>page.getByRole('button',{name:'Paste 1 to move',exact:true}).isVisible(),'Ctrl+X staged the selected copy');
  await page.locator('.breadcrumbs').getByRole('button',{name:'My Drive',exact:true}).click();
  // Moving to root would collide with the original; choose another destination.
  await name(`References-${engine}`).dblclick();await page.keyboard.press('Control+v');
  await until(async()=>await name(`notes-${engine}.txt`).count()===1,'Ctrl+X/V moved the selected file');
  assert.equal((await call(`${base}?parent_uid=${folder.uid}`,user)).items.length,4,'Cut removed only the selected copy');
  await page.locator('.breadcrumbs').getByRole('button',{name:'My Drive',exact:true}).click();
  // Paste an external image through the actual ClipboardEvent path, without permissions or file scans.
  // Hold an unrelated metadata action: uploads must not be discarded by its busy guard.
  let metadataStarted=false;let releaseMetadata;const gate=new Promise(resolve=>releaseMetadata=resolve);
  await page.route('**/drive/items/*/star',async route=>{metadataStarted=true;await gate;await route.continue();});
  await page.getByRole('button',{name:`Star notes-${engine}.txt`,exact:true}).click();await until(()=>metadataStarted,'metadata action is pending');
  try {
    await name(`notes-${engine}.txt`).click();await page.keyboard.press('Control+c');await until(()=>page.getByRole('button',{name:'Paste 1 to copy',exact:true}).isVisible(),'Copy is local even while metadata work is pending');
    assert.equal(await page.getByRole('button',{name:'Copy',exact:true}).isEnabled(),true,'Local Copy stays enabled during metadata writes');assert.equal(await page.getByRole('button',{name:'Cut',exact:true}).isEnabled(),true,'Local Cut stays enabled during metadata writes');
    await page.keyboard.press('Control+x');await until(()=>page.getByRole('button',{name:'Paste 1 to move',exact:true}).isVisible(),'Cut is local even while metadata work is pending');
    const accepted=await page.evaluate(({engine})=>{const data=new DataTransfer();data.items.add(new File([new Uint8Array([137,80,78,71])],`pasted-${engine}.png`,{type:'image/png'}));const event=new ClipboardEvent('paste',{bubbles:true,cancelable:true,clipboardData:data});if(event.clipboardData?.files.length!==data.files.length)Object.defineProperty(event,'clipboardData',{value:data});document.querySelector('.files').dispatchEvent(event);return event.defaultPrevented;},{engine});
    assert.equal(accepted,true,'External file paste is accepted while metadata work is pending');
  } finally {releaseMetadata();}
  await until(async()=>await name(`pasted-${engine}.png`).count()===1,'External clipboard image uploaded');
  await page.unroute('**/drive/items/*/star');
  // Real DataTransfer file drop; folder reader contract is independently unit-tested.
  await page.evaluate(({engine})=>{const data=new DataTransfer();data.items.add(new File(['dropped'],`dropped-${engine}.txt`,{type:'text/plain'}));const area=document.querySelector('.files');area.dispatchEvent(new DragEvent('dragover',{bubbles:true,cancelable:true,dataTransfer:data}));area.dispatchEvent(new DragEvent('drop',{bubbles:true,cancelable:true,dataTransfer:data}));},{engine});
  await until(async()=>await name(`dropped-${engine}.txt`).count()===1,'Dropped file uploaded');
  // Browser folder-entry APIs cannot be constructed by DataTransfer; emulate only the OS entry reader,
  // exercising Drive's real drop handler, hierarchy preparation, HTTP upload and persistent metadata.
  await page.evaluate(({engine})=>{const data=new DataTransfer();data.items.add(new File(['placeholder'],'placeholder.txt'));const leaf={isFile:true,isDirectory:false,name:'inside.txt',file:ok=>ok(new File(['nested'], 'inside.txt',{type:'text/plain'}))};const empty={isFile:false,isDirectory:true,name:'Empty',createReader:()=>({readEntries:ok=>ok([])})};const folder={isFile:false,isDirectory:true,name:`Dropped folder ${engine}`,createReader:()=>{const pages=[[leaf],[empty],[]];return{readEntries:ok=>ok(pages.shift())};}};const descriptor=Object.getOwnPropertyDescriptor(DataTransferItem.prototype,'webkitGetAsEntry');Object.defineProperty(DataTransferItem.prototype,'webkitGetAsEntry',{configurable:true,value:()=>folder});try{document.querySelector('.files').dispatchEvent(new DragEvent('drop',{bubbles:true,cancelable:true,dataTransfer:data}));}finally{if(descriptor)Object.defineProperty(DataTransferItem.prototype,'webkitGetAsEntry',descriptor);else delete DataTransferItem.prototype.webkitGetAsEntry;}},{engine});
  await until(async()=>(await call(base,user)).items.some(i=>i.name===`Dropped folder ${engine}`),'Dropped folder created');
  const dropped=(await call(base,user)).items.find(i=>i.name===`Dropped folder ${engine}`);
  await until(async()=>{const items=(await call(`${base}?parent_uid=${dropped.uid}`,user)).items;return items.some(i=>i.name==='inside.txt')&&items.some(i=>i.name==='Empty');},'Dropped nested file and empty folder preserved');
  await page.getByRole('button',{name:'Refresh Drive',exact:true}).click();
  await page.setViewportSize({width:1600,height:1000});
  const alignment=await page.evaluate(()=>{const h=document.querySelector('.file-head');const r=document.querySelector('.file-row');return{header:[...h.children].map(n=>n.getBoundingClientRect().left),row:[...r.children].map(n=>n.getBoundingClientRect().left)};});
  assert.equal(alignment.header.length,alignment.row.length);alignment.header.forEach((x,i)=>assert.ok(Math.abs(x-alignment.row[i])<2,`Column ${i} is aligned`));
  await page.screenshot({path:join(directory,`${engine}-drive-file-manager.png`),fullPage:true});
  await page.getByRole('button',{name:`More actions for preview-${engine}.png`,exact:true}).click();await menu.getByRole('menuitem',{name:'Versions and activity',exact:true}).click();
  await page.getByRole('tab',{name:'Activity',exact:true}).click();await page.locator('.activity-list .activity').first().waitFor();
  assert.equal(await page.locator('.drive-modal .version').count(),0,'Activity is separated from versions');
  await page.screenshot({path:join(directory,`${engine}-drive-activity.png`)});await page.getByRole('button',{name:'Close dialog',exact:true}).click();
  await name(`notes-${engine}.txt`).click();await page.keyboard.press('Control+a');assert.equal(await page.locator('.file-row.chosen').count(),await rows.count());
  await page.getByRole('button',{name:'Clear selection',exact:true}).click();await page.setViewportSize({width:390,height:850});
  await name(`preview-${engine}.png`).click({button:'right'});await menu.waitFor();const bounds=await menu.boundingBox();assert.ok(bounds.x>=0&&bounds.x+bounds.width<=391&&bounds.y>=0,'Context menu is mobile viewport-clamped');await page.screenshot({path:join(directory,`${engine}-drive-menu-mobile.png`)});await page.keyboard.press('Escape');
  await page.setViewportSize({width:1280,height:900});
  // Non-Drive context menus must remain native, including after unmount.
  await name(`notes-${engine}.txt`).click();await page.keyboard.press('Control+c');
  await page.evaluate(()=>location.hash='collaboration');await until(async()=>await page.locator('.drive').count()===0,'Drive unmounted');
  assert.equal(await page.evaluate(()=>{const e=new MouseEvent('contextmenu',{bubbles:true,cancelable:true});document.body.dispatchEvent(e);return e.defaultPrevented;}),false,'Right-click is not globally overridden');
  await page.evaluate(()=>location.hash='drive');await name(`preview-${engine}.png`).waitFor();await page.getByRole('button',{name:'Paste 1 to copy',exact:true}).waitFor();await page.keyboard.press('Escape');
}
