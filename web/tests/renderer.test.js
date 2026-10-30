import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {SpatialIndex,BudgetCache,CHUNK_SIZE,CACHE_BYTES,chunkResolution,viewportResolution,cachedChunk,SpriteRenderer,buildSpriteScene} from '../renderer.js';
import {transportLayout} from '../transport.js';

const graphics=JSON.parse(readFileSync(new URL('../entity-graphics.json',import.meta.url),'utf8'));
const entity=(id,name,x,y,direction=4)=>({entity_number:id,name,position:{x,y},direction});
const makeFactory=entities=>({request:{belt:'transport-belt'},document:{blueprint:{entities}},entity_sizes:{'transport-belt':[1,1],'assembling-machine-2':[3,3],'fast-inserter':[1,1],'small-electric-pole':[1,1]}});
const scene=(entities,catalog={transport_sprites:['transport-belt'],entity_graphics:graphics})=>{
  const factory=makeFactory(entities);
  return buildSpriteScene(factory,catalog,transportLayout(entities,'transport-belt'));
};

test('viewport culling handles negative coordinates, avoids duplicates and preserves painting order',()=>{
  const items=[{name:'large',bounds:{x:-20,y:-20,w:50,h:50}},{name:'near',bounds:{x:-2,y:-2,w:1,h:1}},{name:'far',bounds:{x:100,y:100,w:1,h:1}}];
  const index=new SpatialIndex(items);
  assert.deepEqual(index.query({x:-4,y:-4,w:8,h:8}).map(item=>item.name),['large','near']);
  assert.deepEqual(index.query({x:99,y:99,w:4,h:4}).map(item=>item.name),['far']);
  assert.equal(index.hit({x:-1.5,y:-1.5}).name,'near');
  assert.equal(index.hit({x:80,y:80}),null);
});

test('cache evicts the least recently used chunks and respects its byte budget on replacement',()=>{
  const cache=new BudgetCache(12);
  for(const key of ['a','b','c'])cache.set(key,key,4);
  assert.equal(cache.get('a'),'a');cache.set('d','d',4);
  assert.equal(cache.get('b'),undefined);
  assert.equal(cache.bytes,12);
  cache.set('a','large-a',8);
  assert.equal(cache.bytes,12);
  assert.equal(cache.get('c'),undefined);
  cache.set('oversized','not-cached',100);
  assert.equal(cache.get('oversized'),undefined);
  cache.clear();assert.equal(cache.bytes,0);
});

test('quality adapts so the visible working set fits in cache instead of thrashing at every pan',()=>{
  assert.equal(chunkResolution(1.2,8),2);
  assert.equal(chunkResolution(100,4),64);
  for(const visible of [8,64,400,2000]){
    const scale=chunkResolution(40,visible);
    assert.ok(visible*(CHUNK_SIZE*scale+2)**2*4<=CACHE_BYTES*.75);
  }
});

test('panning across positive and negative chunk boundaries keeps the same cache resolution',()=>{
  const view={x:0,y:0,w:100,h:70},scale=viewportResolution(view,3200);
  for(const x of [-16.1,-16,-.01,0,.01,15.99,16,16.01,100]){
    assert.equal(viewportResolution({...view,x,y:x/2},3200),scale);
  }
});

test('zoom refinement can reuse both lower and higher resolution chunks',()=>{
  const cache=new BudgetCache();
  cache.set('8:-1,2','coarse',100);cache.set('32:-1,2','sharp',100);
  assert.deepEqual(cachedChunk(cache,-1,2,16),{tile:'coarse',scale:8});
  assert.deepEqual(cachedChunk(cache,-1,2,4),{tile:'coarse',scale:8});
  assert.deepEqual(cachedChunk(cache,-1,2,32),{tile:'sharp',scale:32});
  assert.equal(cachedChunk(cache,0,2,16),null);
});

test('an exhausted refinement budget still paints every visible chunk at its world position',()=>{
  const draws=[],transforms=[],canvas={width:640,height:320};
  const renderer=Object.assign(Object.create(SpriteRenderer.prototype),{
    ready:true,view:{x:-8.25,y:3.75,w:32,h:16},width:640,height:320,canvas,
    context:{setTransform:(...args)=>transforms.push(args),clearRect(){},drawImage:(...args)=>draws.push(args)},
    index:{visible:()=>[[-1,0],[0,0],[1,0]]},cache:new BudgetCache(),bounds:new Map(),node:{dataset:{}},
    chunk:(x,y,scale)=>({x,y,scale}),schedule(){this.scheduled=true;}
  });
  const originalPerformance=globalThis.performance,originalDpr=globalThis.devicePixelRatio;
  let tick=0;
  try{
    globalThis.devicePixelRatio=1;
    globalThis.performance={now:()=>tick+=10,getEntriesByName:()=>[],measure(){}};
    renderer.draw();
    assert.equal(draws.length,3);
    assert.deepEqual(draws.map(args=>args.slice(-4)),[[-16,0,16,16],[0,0,16,16],[16,0,16,16]]);
    assert.deepEqual(transforms.at(-1),[20,0,0,20,165,-75]);
    assert.equal(draws[1][0].scale,1);
    assert.equal(renderer.scheduled,true);
    assert.equal(renderer.node.dataset.pendingChunks,'1');
  }finally{
    globalThis.performance=originalPerformance;
    if(originalDpr===undefined)delete globalThis.devicePixelRatio;else globalThis.devicePixelRatio=originalDpr;
  }
});

test('a hundred belt tiles reuse three cropped frames and offscreen cells are not queried',()=>{
  const entities=Array.from({length:100},(_,i)=>entity(i+1,'transport-belt',i+.5,.5));
  const plan=scene(entities),index=new SpatialIndex(plan.commands);
  assert.equal(plan.frames.size,3); // straight, start, end
  assert.equal(plan.bounds.size,100);
  assert.ok(index.query({x:40,y:0,w:4,h:1}).length<20);
  for(const frame of plan.frames.values()){assert.equal(frame.sw,128);assert.equal(frame.sh,128);}
});

test('sprite hit testing retains raised poles and hands above neighbouring buildings',()=>{
  const plan=scene([entity(1,'fast-inserter',5,5,0),entity(2,'assembling-machine-2',5,7,0),entity(3,'small-electric-pole',10,5,0)]);
  const hits=new SpatialIndex(plan.hits);
  assert.equal(hits.hit({x:5,y:6.1}).entity,1);
  assert.equal(hits.hit({x:10,y:3}).entity,3);
  assert.equal(hits.hit({x:5,y:7.5}).entity,2);
  assert.ok(plan.bounds.get(3).top<3);
  assert.deepEqual(plan.bounds.get(3).footprint,{x:9.5,y:4.5,w:1,h:1});
});

test('unavailable game graphics remain eligible for the existing SVG fallback',()=>{
  const plan=scene([entity(1,'transport-belt',0,0)],{transport_sprites:[]});
  assert.equal(plan.commands.length,0);assert.equal(plan.bounds.size,0);
});
