import test from 'node:test';
import assert from 'node:assert/strict';
import {transportLayout,transportRotation,transportDevice,transportSpriteFrames,transportSymbols,transportTile,beltSpriteLayers} from '../transport.js';

const vectors={0:[0,-1],4:[1,0],8:[0,1],12:[-1,0]};
const entity=(id,x,y,direction,name='belt',type)=>({entity_number:id,name,position:{x,y},direction,type});
const layout=entities=>transportLayout(entities,'belt',{splitter:'splitter',underground:'underground'});

test('all eight corner orientations follow a single perpendicular incoming belt',()=>{
  for(const direction of [0,4,8,12])for(const entry of [(direction+4)%16,(direction+12)%16]){
    const [x,y]=vectors[entry];
    const belts=layout([entity(1,0,0,direction),entity(2,x,y,(entry+8)%16)]).belts;
    assert.deepEqual(belts.get(1),{direction,entry,turn:true,arrow:false,start:false,end:true});
  }
});
test('rear-fed belts remain straight when another belt side-loads a lane',()=>{
  const belts=layout([entity(1,0,0,4),entity(2,-1,0,4),entity(3,0,-1,8)]).belts;
  assert.deepEqual(belts.get(1),{direction:4,entry:12,turn:false,arrow:true,start:false,end:true});
});
test('facing neighbours and two-sided loading do not create false corners',()=>{
  assert.equal(layout([entity(1,0,0,4),entity(2,0,-1,0),entity(3,1,0,12)]).belts.get(1).turn,false);
  assert.equal(layout([entity(1,0,0,4),entity(2,0,-1,8),entity(3,0,1,0)]).belts.get(1).turn,false);
  assert.equal(layout([entity(1,0,0,4)]).belts.get(1).turn,false);
});
test('underground outputs feed visible corners while entrances do not',()=>{
  const belt=entity(1,0,0,0);
  assert.equal(layout([belt,entity(2,-1,0,4,'underground','output')]).belts.get(1).entry,12);
  assert.equal(layout([belt,entity(2,-1,0,4,'underground','input')]).belts.get(1).turn,false);
});
test('both rotated splitter outputs can feed corners',()=>{
  const belts=layout([entity(1,-1,.5,4,'splitter'),entity(2,0,0,0),entity(3,0,1,8)]).belts;
  assert.equal(belts.get(2).entry,12);
  assert.equal(belts.get(3).entry,12);
  assert.equal(belts.get(2).turn,true);
  assert.equal(belts.get(3).turn,true);
});
test('tunnels connect to the nearest aligned exit facing the same way',()=>{
  const entities=[entity(1,0,0,4,'underground','input'),entity(2,3,1,4,'underground','output'),
    entity(3,2,0,12,'underground','output'),entity(4,5,0,4,'underground','output'),entity(5,8,0,4,'underground','output')];
  assert.equal(layout(entities).tunnels.length,1);
  assert.equal(layout(entities).tunnels[0].output.entity_number,4);
  assert.equal(layout([entity(1,0,0,4,'underground','input')]).tunnels.length,0);
});

test('long straight runs show sparse direction markers instead of one per tile',()=>{
  const entities=Array.from({length:24},(_,i)=>entity(i+1,i+.5,.5,4));
  const arrows=[...layout(entities).belts.values()].filter(belt=>belt.arrow);
  assert.equal(arrows.length,6);
  assert.equal(layout([entity(1,.5,.5,4)]).belts.get(1).arrow,true);
});

// Device art must live in the same local space as belts: +x is the flow, so
// the splitter's two lanes are separated across y and the icon never turns a
// quarter turn away from the belts it feeds.
const fakeTag=tag=>({tag,attributes:{},children:[],setAttribute(key,value){this.attributes[key]=String(value);},append(...nodes){this.children.push(...nodes);}});
const fakeSvg=(tag,attributes={},content)=>Object.assign(fakeTag(tag),{attributes:{...attributes},content});
const all=(node,predicate,found=[])=>{if(predicate(node))found.push(node);for(const child of node.children||[])all(child,predicate,found);return found;};
const device=(e,overrides={})=>transportDevice(fakeSvg,e,{splitter:'splitter',underground:'underground',belt:'transport-belt',w:1,h:2,...overrides});

test('belt and device rotations share one origin',()=>{
  assert.equal(transportRotation(4),0);
  assert.equal(transportRotation(8),90);
  assert.equal(transportRotation(0),-90);
  assert.equal(transportRotation(12),180);
  for(const direction of [0,4,8,12]){
    const [group]=device(entity(1,0,0,direction,'splitter'));
    assert.match(group.attributes.transform,/rotate\((-?[\d.]+)\)/);
    assert.equal(Number(group.attributes.transform.match(/rotate\((-?[\d.]+)\)/)[1]),transportRotation(direction));
  }
});

test('splitter art fills its footprint with two lanes across the flow',()=>{
  const [group,hitbox]=device(entity(1,0,0,4,'splitter'),{w:1,h:2});
  const lanes=all(group,node=>node.tag==='use'&&node.attributes.href==='#belt-straight').map(node=>node.attributes.y);
  assert.deepEqual(lanes,[-1,0]);
  assert.equal(hitbox.attributes.width,1-.04);
  assert.equal(hitbox.attributes.height,2-.04);
});

test('underground art keeps one lane and mirrors its hood for the exit',()=>{
  const [input]=device(entity(1,0,0,4,'underground','input'),{w:1,h:2});
  const [output]=device(entity(2,0,0,4,'underground','output'),{w:1,h:2});
  const lanes=node=>all(node,item=>item.tag==='use'&&item.attributes.href==='#belt-straight').length;
  assert.equal(lanes(input),1);
  assert.equal(lanes(output),1);
  const mirrored=node=>all(node,item=>item.attributes?.transform==='scale(-1 1)').length;
  assert.equal(mirrored(input),0);
  assert.equal(mirrored(output),1);
});

test('game underground sprites select the flow direction and a distinct entrance or exit',()=>{
  const name='fast-underground-belt',assets=[`${name}-structure`];
  for(const direction of [0,4,8,12]){
    const [input]=transportSpriteFrames(entity(1,0,0,direction,name,'input'),assets);
    const [output]=transportSpriteFrames(entity(2,0,0,direction,name,'output'),assets);
    assert.equal(input.sx,direction/4*192);
    assert.equal(output.sx,((direction/4+2)%4)*192);
    assert.equal(input.sy,192);
    assert.equal(output.sy,0);
  }
});

test('game splitters use directional frames and include the raised east/west patch',()=>{
  const name='fast-splitter',assets=['north','east','south','west','east-top_patch','west-top_patch'].map(v=>`${name}-${v}`);
  for(const [direction,facing] of [[0,'north'],[4,'east'],[8,'south'],[12,'west']]){
    const frames=transportSpriteFrames(entity(1,0,0,direction,name),assets);
    assert.equal(frames[0].asset,`${name}-${facing}`);
    assert.equal(frames.length,[4,12].includes(direction)?2:1);
    const [,art,hitbox]=device(entity(1,0,0,direction,name),{splitter:name,sprites:assets});
    assert.equal(art.attributes.transform,'translate(0 0)'); // No rotation of perspective art.
    assert.equal(all(art,node=>node.tag==='image').length,frames.length);
    assert.equal(hitbox.attributes.class,'entity-body transport-hitbox');
  }
});

test('missing sprite layers fall back to a complete schematic',()=>{
  assert.deepEqual(transportSpriteFrames(entity(1,0,0,4,'fast-splitter'),['fast-splitter-east']),[]);
  assert.deepEqual(transportSpriteFrames(entity(1,0,0,4,'mod-underground-belt','input'),[]),[]);
});

test('turbo splitters use their own sprite dimensions and patch offsets',()=>{
  const [body,patch]=transportSpriteFrames(entity(1,0,0,4,'turbo-splitter'),['turbo-splitter-east','turbo-splitter-east-top_patch']);
  assert.equal(body.width,86);
  assert.equal(body.height,84);
  assert.equal(body.x,5/32-86/128);
  assert.equal(patch.height,102);
  assert.equal(patch.x,4/32-90/128);
  assert.equal(patch.y,-19.5/32-102/128);
});

test('straight tiles and corners both mark their connection boundaries',()=>{
  const defs=transportSymbols(fakeSvg,'transport-belt');
  for(const id of ['belt-straight','belt-turn']){
    const symbol=defs.children.find(node=>node.attributes.id===id);
    const seams=all(symbol,node=>node.attributes.class==='belt-tile-seam');
    assert.equal(seams.length,1);
    assert.equal(seams[0].attributes['stroke-width'],.025);
    assert.equal(seams[0].attributes.d.split('M ').length-1,2);
  }
});

test('original corner frames match all eight entry and exit sides without rotation',()=>{
  for(const [entry,direction,index] of [[4,0,5],[0,4,6],[12,0,7],[0,12,8],[8,4,9],[4,8,10],[8,12,11],[12,8,12]]){
    const shape={direction,entry,turn:true,start:false,end:false};
    assert.deepEqual(beltSpriteLayers(shape),[{index,x:0,y:0}]);
    const [art]=transportTile(fakeSvg,entity(1,2,3,direction,'transport-belt'),shape,['transport-belt']);
    assert.equal(art.attributes.transform,'translate(2 3)');
    assert.equal(art.children[0].attributes.href,`#belt-game-${index}`);
    assert.equal(all(art,node=>node.attributes.class==='belt-direction').length,0);
  }
});

test('original straight runs and their caps align at the vacant neighbouring tile edges',()=>{
  for(const [direction,straight,start,end] of [[0,3,13,18],[4,1,15,20],[8,4,17,14],[12,2,19,16]]){
    const [dx,dy]=vectors[direction];
    assert.deepEqual(beltSpriteLayers({direction,turn:false,start:true,end:true}),[
      {index:straight,x:0,y:0},{index:start,x:-dx,y:-dy},{index:end,x:dx,y:dy}
    ]);
  }
  const belts=layout([entity(1,0,0,4),entity(2,1,0,4),entity(3,2,0,4)]).belts;
  assert.deepEqual([...belts.values()].map(({start,end})=>[start,end]),[[true,false],[false,false],[false,true]]);
});

test('game belt caps stop at underground entrances and both splitter lanes',()=>{
  let belts=layout([entity(1,0,0,4),entity(2,1,0,4,'underground','input'),
    entity(3,4,0,4,'underground','output'),entity(4,5,0,4)]).belts;
  assert.equal(belts.get(1).end,false);
  assert.equal(belts.get(4).start,false);
  belts=layout([entity(1,0,0,4),entity(2,0,1,4),entity(3,1,.5,4,'splitter'),entity(4,2,0,4),entity(5,2,1,4)]).belts;
  assert.equal(belts.get(1).end,false);
  assert.equal(belts.get(2).end,false);
  assert.equal(belts.get(4).start,false);
  assert.equal(belts.get(5).start,false);
});

test('game atlases share one image and crop the first frame at each tier’s actual width',()=>{
  for(const [name,columns] of [['transport-belt',16],['fast-transport-belt',32],['express-transport-belt',32],['turbo-transport-belt',64]]){
    const defs=transportSymbols(fakeSvg,name,[name]);
    const atlas=all(defs,node=>node.tag==='image');
    assert.equal(atlas.length,1);
    assert.equal(atlas[0].attributes.width,columns*128);
    const last=defs.children.find(node=>node.attributes.id==='belt-game-20');
    assert.equal(last.children[0].attributes.viewBox,'0 2432 128 128');
    assert.equal(last.children[0].children[0].attributes.href,'#belt-game-atlas');
  }
  assert.equal(all(transportSymbols(fakeSvg,'transport-belt'),node=>node.tag==='image').length,0);
});

test('devices use the same original belt texture under their directional structure',()=>{
  const name='fast-splitter',sprites=['fast-transport-belt',`${name}-east`,`${name}-east-top_patch`];
  const [base]=device(entity(1,0,0,4,name),{splitter:name,belt:'fast-transport-belt',sprites});
  assert.equal(base.attributes.transform,'translate(0 0)');
  assert.deepEqual(base.children.map(node=>[node.attributes.href,node.attributes.x,node.attributes.y]),[
    ['#belt-game-1',-1,-1.5],['#belt-game-1',-1,-.5]
  ]);
});
