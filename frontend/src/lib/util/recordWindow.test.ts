import { describe, expect, it } from 'vitest';
import { mergeWindow, windowPieces, windowAnchor } from './recordWindow';
describe('bounded record windows', () => {
  const rows = Array.from({length:500}, (_, index) => ({uid:String(index)}));
  it('renders a viewport and overscan, not the whole cached window', () => {
    const pieces=windowPieces(rows.map(row=>row.uid),{},5000,600,50);
    expect(pieces.filter(piece=>piece.kind==='row')).toHaveLength(29);
    expect(pieces.filter(piece=>piece.kind==='gap')).toHaveLength(2);
    expect(pieces.reduce((sum,piece)=>sum+(piece.kind==='gap'?piece.height:50),0)).toBe(25000);
  });
  it('retains an offscreen editor without rendering intervening rows', () => {
    const pieces=windowPieces(rows.map(row=>row.uid),{},5000,600,50,['1']);
    expect(pieces.some(piece=>piece.kind==='row'&&piece.key==='1')).toBe(true);
    expect(pieces.filter(piece=>piece.kind==='row').length).toBeLessThan(35);
  });
  it('uses measured variable heights and preserves total spacing', () => {
    const pieces=windowPieces(['a','b','c'],{a:100,b:200,c:300},150,20,50,[],0);
    expect(pieces).toEqual([{kind:'gap',height:100,key:'gap:a'},{kind:'row',index:1,key:'b'},{kind:'gap',height:300,key:'gap:c'}]);
  });
  it('anchors a fast scroll into an unrendered spacer rather than an offscreen row', () => {
    expect(windowAnchor(['a','b','c','d'],{a:100,b:200},325,50)).toEqual({uid:'c',index:2,offset:-25});
    expect(windowAnchor(['a','b'],{},50,50)).toEqual({uid:'b',index:1,offset:0});
    expect(windowAnchor(['a'],{},-20,50)).toEqual({uid:'a',index:0,offset:20});
    expect(windowAnchor([],{},100,50)).toBeUndefined();
  });
  it('evicts the opposite edge, deduplicates, and can navigate back', () => {
    const next=mergeWindow(rows.slice(0,100),rows.slice(90,160),false,100)!;
    expect(next.rows).toEqual(rows.slice(60,160));
    expect(next.added).toBe(60);
    expect(mergeWindow(next.rows,rows.slice(0,60),true,100)!.rows).toEqual(rows.slice(0,100));
  });
  it('never destroys a pinned editor to satisfy an edge request', () => {
    expect(mergeWindow(rows.slice(0,100),rows.slice(100,150),false,100,['1'])).toBeNull();
  });
});
