import { expect, it } from 'vitest';
import { UploadSlots } from './uploadSlots';
it('bounds slots, serves waiters fairly, and handles repeated release and cancellation',async()=>{
  const slots=new UploadSlots(2);const first=await slots.acquire(),second=await slots.acquire();
  const controller=new AbortController();const rejected=expect(slots.acquire(controller.signal)).rejects.toMatchObject({name:'AbortError'});
  const entered:string[]=[];const third=slots.acquire().then(release=>{entered.push('third');return release;}),fourth=slots.acquire().then(release=>{entered.push('fourth');return release;});
  controller.abort();await rejected;first();first();const endThird=await third;expect(entered).toEqual(['third']);second();const endFourth=await fourth;expect(entered).toEqual(['third','fourth']);endThird();endFourth();
  const aborted=new AbortController();aborted.abort();await expect(slots.acquire(aborted.signal)).rejects.toMatchObject({name:'AbortError'});
});
