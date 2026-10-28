// Optional diagnostic boundary only. Never claim this proxy run as qualification.
// Preserve bytes, limiting/durability headers and TLS verification to owned N1.
import https from 'node:https';
import {readFile,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
export async function startN1Trace(directory,target){
 const cert=await readFile(join(directory,'cert.pem')),key=await readFile(join(directory,'key.pem'));
 const agent=new https.Agent({ca:cert,keepAlive:true,maxSockets:1024});
 const entries=[],base=new URL(target);
 const server=https.createServer({cert,key},(incoming,outgoing)=>{
  const url=new URL(incoming.url,target),objectKey=url.searchParams.get('object_key');
  const entry={method:incoming.method,operation:url.pathname.replace(/(\/durable\/)(?!init(?:$|\/))[^/]+/,'$1:version'),objectKeyHash:objectKey?createHash('sha256').update(objectKey).digest('hex'):undefined,startedAtMs:Date.now()};entries.push(entry);
  const upstream=https.request(url,{method:incoming.method,agent,headers:{...incoming.headers,host:base.host}},response=>{
   entry.status=response.statusCode;entry.headersAtMs=Date.now();outgoing.writeHead(response.statusCode,response.headers);response.pipe(outgoing);
   response.on('end',()=>entry.finishedAtMs=Date.now());response.on('error',()=>{entry.error='upstream response interrupted';outgoing.destroy();});
  });
  upstream.on('error',()=>{entry.error='upstream request interrupted';entry.finishedAtMs=Date.now();if(!outgoing.headersSent)outgoing.writeHead(502);outgoing.end();});
  incoming.on('aborted',()=>upstream.destroy());outgoing.on('close',()=>{if(!outgoing.writableFinished)upstream.destroy();});incoming.pipe(upstream);
 });
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 return {url:`https://127.0.0.1:${server.address().port}`,async close(){
  agent.destroy();const closed=new Promise(resolve=>server.close(resolve));server.closeAllConnections();await closed;
  await writeFile(join(directory,'upstream-timings.json'),JSON.stringify({diagnosticOnly:true,entries},null,2),{mode:0o600});
 }};
}
