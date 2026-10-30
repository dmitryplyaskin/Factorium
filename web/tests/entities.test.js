import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {frameBounds,entitySymbols,entitySprite,wirePoint,inserterPose} from '../entities.js';

const graphics=JSON.parse(readFileSync(new URL('../entity-graphics.json',import.meta.url),'utf8'));
const entity=(name,direction=0,x=5,y=6)=>({entity_number:7,name,direction,position:{x,y}});
const svg=(tag,attributes={})=>({tag,attributes,children:[],append(...children){this.children.push(...children);},setAttribute(key,value){this.attributes[key]=value;}});

test('sprite geometry preserves game scale and world-space shifts',()=>{
  assert.deepEqual(frameBounds({width:200,height:188,scale:.5,shift:[0,.078125]}),{x:-1.5625,y:-1.390625,width:3.125,height:2.9375});
  const sprite=entitySprite(svg,entity('small-electric-pole'),graphics,1,1);
  assert.ok(sprite.bounds.top<3); // The top is above its footprint at y=6.
  assert.equal(sprite.hitbox.attributes.y,5.54); // Selection remains on the base tile.
  assert.equal(sprite.body.attributes.transform,'translate(5 6)');
  assert.equal(sprite.body.attributes['pointer-events'],'visiblePainted');
});

test('copper wires attach to the real pole top in each directional frame',()=>{
  for(const direction of [0,4,8,12]){
    const e=entity('small-electric-pole',direction),offset=graphics.entities[e.name].directions[direction].wire;
    assert.deepEqual(wirePoint(e,graphics),{x:5+offset[0],y:6+offset[1]});
    assert.ok(wirePoint(e,graphics).y<4);
  }
  assert.deepEqual(wirePoint(entity('mod-pole'),graphics),{x:5,y:6});
});

test('static inserter hands point at the drop side in all four directions',()=>{
  for(const [direction,x,y] of [[0,5,7.2],[4,3.8,6],[8,5,4.8],[12,6.2,6]]){
    const pose=inserterPose(entity('fast-inserter',direction),graphics);
    assert.ok(Math.abs(pose.drop.x-x)<1e-9);
    assert.ok(Math.abs(pose.drop.y-y)<1e-9);
    assert.equal(pose.reach,1.2);
  }
  assert.equal(inserterPose(entity('long-handed-inserter'),graphics).reach,2.2);
});

test('layers keep shadows separate and draw original arms above the body',()=>{
  const assembler=entitySprite(svg,entity('assembling-machine-2'),graphics,3,3);
  assert.equal(assembler.body.children.length,2);
  assert.equal(assembler.shadow.children.length,1);
  assert.equal(assembler.arm.children.length,0);
  const inserter=entitySprite(svg,entity('long-handed-inserter',8),graphics,1,1);
  assert.equal(inserter.arm.children[0].attributes.transform,'rotate(360)');
  assert.equal(inserter.arm.children[0].children.length,2);
  assert.equal(inserter.arm.attributes['data-entity'],7);
  assert.equal(inserter.arm.attributes['pointer-events'],'visiblePainted');
  assert.equal(entitySprite(svg,entity('mod-machine'),graphics,3,3),null);
});

test('shared images load only for entities actually present on the blueprint',()=>{
  const e=entity('assembling-machine-2'),defs=entitySymbols(svg,graphics,[e,e]);
  assert.equal(defs.children.length,3);
  assert.equal(new Set(defs.children.map(node=>node.attributes.href)).size,3);
  assert.equal(entitySymbols(svg,graphics,[entity('mod-machine')]).children.length,0);
});
