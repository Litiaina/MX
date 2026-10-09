import { expect, it } from 'vitest';
import { UploadMeter, uploadEta } from './uploadProgress';
it('reports live in-flight bytes, speed and ETA without counting resumed bytes as speed',()=>{
  let now=0; const meter=new UploadMeter(1000,400,()=>now);
  expect(meter.snapshot()).toEqual({loaded:400,bytesPerSecond:0,etaSeconds:null});
  now=1000; meter.part(1,200,400);
  expect(meter.snapshot()).toEqual({loaded:600,bytesPerSecond:200,etaSeconds:2});
  meter.stored(1,400); now=2000;
  expect(meter.snapshot()).toEqual({loaded:800,bytesPerSecond:200,etaSeconds:1});
  meter.part(2,5000,200); now=3000;
  expect(meter.snapshot()).toEqual({loaded:1000,bytesPerSecond:200,etaSeconds:0});
});
it('handles retries, parallel slots, stalls and zero length without impossible progress',()=>{
  let now=0; const meter=new UploadMeter(1000,0,()=>now);
  meter.part(0,200,500); meter.part(1,100,500); now=1000;
  expect(meter.snapshot().loaded).toBe(300);
  meter.part(0,0,500); now=2000; expect(meter.snapshot().loaded).toBe(100);
  meter.part(0,500,500); meter.stored(0,500); meter.stored(1,500); now=3000;
  expect(meter.snapshot().loaded).toBe(1000);
  now=9000; expect(meter.snapshot().bytesPerSecond).toBe(0);
  expect(new UploadMeter(0).snapshot().etaSeconds).toBeNull();
  expect(uploadEta(61)).toBe('1m 1s remaining'); expect(uploadEta(3600)).toBe('1h 0m remaining');
});
