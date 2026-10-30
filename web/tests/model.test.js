import test from 'node:test';
import assert from 'node:assert/strict';
import {sameRequest,chooseMachine,verificationState,power,label,outputPorts,inputPorts,inputRequirements,recipeIngredients} from '../model.js';

const request={product:'iron-gear-wheel',rate:5,recipe:'iron-gear-wheel',machine:'assembling-machine-2',belt:'transport-belt',inserter:'fast-inserter',pole:'small-electric-pole'};
test('verification only belongs to the exact recipe, rate and equipment snapshot',()=>{
  assert.equal(sameRequest(request,{...request,rate:'5'}),true);
  for(const key of ['product','recipe','machine','belt','inserter','secondary_inserter','pole'])assert.equal(sameRequest(request,{...request,[key]:'changed'}),false);
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

test('shared outputs combine rates once and retain separate lane requirements',()=>{
  const output={entity_number:20,position:{x:10,y:5},item:'iron-gear-wheel'};
  const factory={rows:[{output:{...output,rate:6,lanes:'right'}},{output:{...output,rate:4,lanes:'left'}},{output:{...output,entity_number:40,rate:3,lanes:'right'}}]};
  const ports=outputPorts(factory);
  assert.equal(ports.length,2);
  assert.equal(ports[0].rate,10);
  assert.equal(ports[0].left_rate,4);
  assert.equal(ports[0].right_rate,6);
  assert.equal(ports[0].lanes,'both');
  assert.deepEqual(ports[0].rows,[1,2]);
  assert.equal(ports[1].lanes,'right');
  assert.equal(factory.rows[0].output.rate,6);
});

test('two ingredients retain distinct ports, demands and row associations',()=>{
  const a={item:'iron-plate',rate:1.05,entity_number:10},b={item:'copper-cable',rate:3.15,entity_number:20};
  const requirements=[{item:a.item,per_second:1,supplied_per_second:a.rate},{item:b.item,per_second:3,supplied_per_second:b.rate}];
  const factory={inputs:requirements,rows:[{input:a,additional_inputs:[b]},{input:{...a,entity_number:30},additional_inputs:[{...b,entity_number:40}]}]};
  assert.deepEqual(inputRequirements(factory),requirements);
  const ports=inputPorts(factory);assert.equal(ports.length,4);assert.deepEqual(ports.map(p=>p.ingredient_index),[0,1,0,1]);assert.deepEqual(ports.map(p=>p.row_index),[1,1,2,2]);
  assert.equal(inputPorts({rows:[{input:a}]}).length,1);
  assert.equal(inputRequirements({input_item:'iron-plate',input_per_second:2,supplied_input_per_second:2.1,request}).length,1);
  assert.equal(recipeIngredients({ingredients:requirements}).length,2);
});

test('generation options invalidate snapshots while absent fields retain defaults',()=>{
  assert.equal(sameRequest(request,{...request,common_input:false,input_markers:false,output_chest:null,max_machines_per_row:null,max_row_width:null}),true);
  for(const [key,value] of Object.entries({common_input:true,reverse_output:true,input_markers:true,output_chest:'steel-chest',max_machines_per_row:4,max_row_width:20}))assert.equal(sameRequest(request,{...request,[key]:value}),false);
  assert.equal(sameRequest({...request,max_row_width:'20'},{...request,max_row_width:20}),true);
});
test('bus exposes only external inlets and their row targets',()=>{
  const source={entity_number:100,item:'iron-plate',direction:8,rate:5.25};
  assert.deepEqual(inputPorts({bus:{lines:[{source,targets:[10,20]}]},rows:[]}),[{...source,targets:[10,20]}]);
});
