import {beforeEach,expect,it,vi} from 'vitest';
const {request}=vi.hoisted(()=>({request:vi.fn()}));
vi.mock('./client',()=>({apiJson:request}));
import {relationshipOptions,relatedRecords} from './relationships';

beforeEach(()=>request.mockReset());

it('encodes linked-field searches and page numbers without interpolating user input',async()=>{
  const signal=new AbortController().signal;
  await relationshipOptions('field/id','Office & Department',2,signal);
  const url=new URL(request.mock.calls[0][0],'https://mx.test');
  expect(url.pathname).toBe('/mx/v1/relationships/field%2Fid/options');
  expect(Object.fromEntries(url.searchParams)).toEqual({q:'Office & Department',page:'2'});
  expect(request.mock.calls[0][1].signal).toBe(signal);
});

it('keeps inbound pagination scoped to the specified module and record',async()=>{
  const signal=new AbortController().signal;
  await relatedRecords('module/id','record/id',3,signal);
  expect(request.mock.calls[0]).toEqual(['/mx/v1/relationships/module%2Fid/record%2Fid/related?page=3',{signal}]);
});
