import test from 'node:test';
import assert from 'node:assert/strict';
import {filterRecipes,recipeGroup,recipeTitle,groupInfo} from '../model.js';

const recipes=[
  {name:'iron-plate',product:'iron-plate',group:'intermediate-products',order:'b'},
  {name:'copper-plate',product:'copper-plate',group:'intermediate-products',order:'a'},
  {name:'electronic-circuit',product:'electronic-circuit',group:'intermediate-products'},
  {name:'transport-belt',product:'transport-belt',group:'logistics'},
  {name:'modded-plate',product:'modded-plate',group:'modded'},
];
test('search matches every word across Russian names and game identifiers',()=>{
  assert.deepEqual(filterRecipes(recipes,' ПЛАСТИНЫ желез ').map(r=>r.name),['iron-plate']);
  assert.deepEqual(filterRecipes(recipes,'iron plate').map(r=>r.name),['iron-plate']);
  assert.deepEqual(filterRecipes(recipes,'iron-plate').map(r=>r.name),['iron-plate']);
  assert.deepEqual(filterRecipes(recipes,'зеленые').map(r=>r.name),['electronic-circuit']);
  assert.deepEqual(filterRecipes(recipes,'железные конвейер'),[]);
});
test('group filters preserve modded groups and missing metadata remains selectable',()=>{
  assert.deepEqual(filterRecipes(recipes,'','logistics').map(r=>r.name),['transport-belt']);
  assert.deepEqual(filterRecipes(recipes,'plate','modded').map(r=>r.name),['modded-plate']);
  assert.equal(groupInfo('modded').label,'modded');
  assert.equal(recipeGroup({name:'old-recipe'}),'other');
  assert.equal(filterRecipes([{name:'old-recipe',product:'old-item'}],'','other').length,1);
});
test('recipe ordering is stable and alternative recipes can be distinguished',()=>{
  assert.deepEqual(filterRecipes(recipes,'пластины').map(r=>r.name),['copper-plate','iron-plate']);
  assert.match(recipeTitle({name:'alternate-smelting',product:'iron-plate'}),/Железные пластины.*alternate-smelting/);
  assert.equal(recipes[0].name,'iron-plate');
});
