import test from 'node:test';
import assert from 'node:assert/strict';
import {sameRequest,chooseMachine,verificationState,power,label} from '../model.js';

const request={product:'iron-gear-wheel',rate:5,recipe:'iron-gear-wheel',machine:'assembling-machine-2',belt:'transport-belt',inserter:'fast-inserter',pole:'small-electric-pole'};
test('verification only belongs to the exact recipe, rate and equipment snapshot',()=>{
  assert.equal(sameRequest(request,{...request,rate:'5'}),true);
  for(const key of ['product','recipe','machine','belt','inserter','pole'])assert.equal(sameRequest(request,{...request,[key]:'changed'}),false);
  assert.equal(sameRequest(request,{...request,rate:6}),false);
  assert.equal(sameRequest(request,{...request,rate:NaN}),false);
  assert.equal(sameRequest(request,null),false);
});
test('changing to a smelting recipe replaces an incompatible assembler',()=>{
  assert.equal(chooseMachine({machines:['electric-furnace']},'assembling-machine-2'),'electric-furnace');
  assert.equal(chooseMachine({machines:['assembling-machine-2','wide-machine']},'wide-machine'),'wide-machine');
  assert.equal(chooseMachine({machines:[]},'assembling-machine-2'),'');
});
test('a complete measurement can fail, and a static check never becomes a game pass',()=>{
  assert.equal(verificationState({status:'not_started'}).tone,'neutral');
  assert.equal(verificationState({status:'complete',report:{passed:false}}).tone,'error');
  assert.equal(verificationState({status:'complete',report:{passed:true}}).tone,'success');
  assert.equal(verificationState({status:'running'}).tone,'warning');
});
test('units and unknown modded names remain readable',()=>{
  assert.match(power(3137600),/МВт/);
  assert.equal(label('a-new-modded-item'),'a-new-modded-item');
});
