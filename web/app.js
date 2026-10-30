import { label, number, power, sameRequest, chooseMachine, verificationState, outputPorts, inputPorts, inputBelts, inputRequirements, recipeIngredients, itemGroups, groupInfo, subgroupLabel, recipeGroup, recipeTitle, filterRecipes } from './model.js';
import { transportLayout, transportSymbols, transportTile, transportDevice, transportPalette, directionLabel } from './transport.js';

const $ = id => document.getElementById(id);
const svgNS = 'http://www.w3.org/2000/svg';
const state = { catalog:null, artifact:null, generating:false, poll:null, clock:null, history:[], view:null, fitted:null, drag:null, selected:null, stringCache:new Map() };
let toastTimer;
let activePanel=null, panelTrigger=null, recipeFilter='all', interfaceHidden=false;

function closePanel(restoreFocus=true) {
  if(!activePanel)return;
  const panel=$(activePanel),focused=panel.contains(document.activeElement);
  panel.hidden=true;activePanel=null;
  delete document.body.dataset.panel;
  for(const button of document.querySelectorAll('[data-panel]'))button.setAttribute('aria-expanded','false');
  if(restoreFocus && focused)panelTrigger?.focus({preventScroll:true});
}
function openPanel(id,trigger) {
  if(interfaceHidden)toggleInterface();
  if(activePanel===id){closePanel();return;}
  closePanel(false);activePanel=id;panelTrigger=trigger || document.querySelector(`[data-panel="${id}"]`);
  $('entity-inspector').hidden=true;
  $(id).hidden=false;panelTrigger?.setAttribute('aria-expanded','true');
  $(id).scrollTop=0;
  document.body.dataset.panel=id;
  $(id).querySelector('button,input,select')?.focus({preventScroll:true});
}
function toggleInterface() {
  interfaceHidden=!interfaceHidden;
  if(interfaceHidden){closePanel(false);$('entity-inspector').hidden=true;}
  document.body.classList.toggle('interface-hidden',interfaceHidden);
  $('toggle-interface').setAttribute('aria-pressed',String(interfaceHidden));
  $('toggle-interface').setAttribute('aria-label',interfaceHidden?'Показать интерфейс':'Скрыть интерфейс');
  $('toggle-interface').title=`${interfaceHidden?'Показать':'Скрыть'} интерфейс · H`;
  if(interfaceHidden)$('viewport').focus({preventScroll:true});
}
function openRecipePicker() {
  if(!state.catalog || state.generating)return;
  if(activePanel!=='config-panel')openPanel('config-panel');
  recipeFilter='all';$('recipe-search').value='';renderRecipePicker();
  $('recipe-dialog').showModal();$('recipe-search').focus();
}
function renderRecipePicker() {
  if(!state.catalog)return;
  const recipes=state.catalog.recipes,query=$('recipe-search').value;
  const groups=[...new Set(recipes.map(recipeGroup))].sort((a,b)=>{
    const order=Object.keys(itemGroups),ai=order.indexOf(a),bi=order.indexOf(b);
    return (ai<0?order.length:ai)-(bi<0?order.length:bi) || a.localeCompare(b);
  });
  const filters=$('recipe-groups'),focusedGroup=document.activeElement?.dataset.group;
  filters.replaceChildren();
  for(const group of ['all',...groups]) {
    const info=group==='all'?{label:'Все',icon:null}:groupInfo(group);
    const button=element('button','recipe-group');button.type='button';button.dataset.group=group;
    button.setAttribute('aria-pressed',String(recipeFilter===group));
    if(info.icon){const img=element('img');img.src=iconURL(info.icon);img.alt='';button.append(img);}
    else button.append(svg('svg',{viewBox:'0 0 24 24'},undefined));
    if(!info.icon)button.firstChild.append(svg('use',{href:'#i-grid'}));
    button.append(element('span','',info.label),element('small','',String(filterRecipes(recipes,query,group).length)));
    button.addEventListener('click',()=>{recipeFilter=group;renderRecipePicker();});filters.append(button);
  }
  if(focusedGroup)filters.querySelector(`[data-group="${CSS.escape(focusedGroup)}"]`)?.focus();
  const matches=filterRecipes(recipes,query,recipeFilter),results=$('recipe-results');results.replaceChildren();
  text('recipe-group-title',recipeFilter==='all'?'Все предметы':groupInfo(recipeFilter).label);
  text('recipe-results-count',`${matches.length} из ${recipes.length}`);
  $('recipe-search-empty').hidden=matches.length>0;results.hidden=!matches.length;
  const grouped=new Map();
  for(const recipe of matches){const group=recipeGroup(recipe),subgroup=recipe.subgroup || 'Прочее';
    if(!grouped.has(group))grouped.set(group,new Map());
    const rows=grouped.get(group);if(!rows.has(subgroup))rows.set(subgroup,[]);rows.get(subgroup).push(recipe);
  }
  for(const group of groups)for(const [subgroup,recipes] of grouped.get(group) || []) {
    const section=element('section','recipe-subgroup');
    section.append(element('h3','',`${recipeFilter==='all'?`${groupInfo(group).label} / `:''}${subgroupLabel(subgroup)}`));
    const grid=element('div','item-grid');
    for(const recipe of recipes) {
      const title=recipeTitle(recipe),button=element('button','item-slot');button.type='button';button.dataset.recipe=recipe.name;
      const selected=recipe.name===$('recipe-select').value;
      button.setAttribute('aria-label',`${title}${recipe.name!==recipe.product?` (${recipe.name})`:''}`);
      button.setAttribute('aria-pressed',String(selected));button.title=`${title}\n${recipe.name}`;
      const img=element('img');img.src=iconURL(recipe.product);img.alt='';img.loading='lazy';
      button.append(img,element('span','item-name',title));
      if(selected)button.append(element('span','item-selected','✓'));
      button.addEventListener('click',()=>{
        $('recipe-select').value=recipe.name;recipeChanged();persist();$('recipe-dialog').close();$('choose-product').focus();
      });grid.append(button);
    }
    section.append(grid);results.append(section);
  }
  const slots=[...results.querySelectorAll('.item-slot')],selected=slots.find(b=>b.getAttribute('aria-pressed')==='true') || slots[0];
  for(const slot of slots)slot.tabIndex=slot===selected?0:-1;
}

function text(id,value) { $(id).textContent=value; }
function svg(tag,attributes={},content) { const e=document.createElementNS(svgNS,tag); for(const [key,value] of Object.entries(attributes)) e.setAttribute(key,String(value)); if(content!==undefined)e.textContent=content; return e; }
function element(tag,className,content) { const e=document.createElement(tag); if(className)e.className=className; if(content!==undefined)e.textContent=content; return e; }
function iconURL(name) { return `/api/icons/${encodeURIComponent(name)}`; }
function toast(message) { clearTimeout(toastTimer);text('toast',message);$('toast').hidden=false;toastTimer=setTimeout(()=>$('toast').hidden=true,3500); }
function showError(id,message) { text(id,message || '');$(id).hidden=!message; }

async function api(path,options={}) {
  const headers={...options.headers};
  if(options.method==='POST'){headers['Content-Type']='application/json';headers['X-Factorium-Request']='1';}
  let response;
  try {response=await fetch(path,{...options,headers});} catch {throw new Error('Нет связи с локальным сервером. Убедитесь, что Factorium запущен.');}
  const contentType=response.headers.get('content-type') || '';
  const body=contentType.includes('application/json') ? await response.json() : await response.text();
  if(!response.ok){const detail=body?.error;const error=new Error(detail ? [detail.message,detail.detail].filter(Boolean).join('\n') : `Ошибка сервера (${response.status})`);error.status=response.status;throw error;}
  return body;
}

function request() {
  const recipe=state.catalog?.recipes.find(r=>r.name===$('recipe-select').value);
  return {product:recipe?.product || '',rate:Number($('rate').value),recipe:recipe?.name || null,machine:$('machine').value,belt:$('belt').value,inserter:$('inserter').value,secondary_inserter:recipeIngredients(recipe).length>=2?$('secondary-inserter').value:null,pole:$('pole').value,common_input:$('common-input').checked,reverse_output:$('reverse-output').checked,output_chest:$('store-output').checked?$('output-chest').value:null,input_markers:$('input-markers').checked,max_machines_per_row:$('limit-machines').checked && $('max-machines').value?Number($('max-machines').value):null,max_row_width:$('limit-row-width').checked && $('max-row-width').value?Number($('max-row-width').value):null};
}
function restoreOptions(r={}) {
  r=r || {};
  $('common-input').checked=Boolean(r.common_input);$('input-markers').checked=Boolean(r.input_markers);$('reverse-output').checked=Boolean(r.reverse_output);
  $('store-output').checked=Boolean(r.output_chest);if(r.output_chest)$('output-chest').value=r.output_chest;
  $('limit-machines').checked=Boolean(r.max_machines_per_row);$('max-machines').value=r.max_machines_per_row || 8;$('limit-row-width').checked=Boolean(r.max_row_width);$('max-row-width').value=r.max_row_width || 20;optionsChanged();
}
function optionsChanged() {
  $('storage-equipment').hidden=!$('store-output').checked;$('output-chest').disabled=!$('store-output').checked;
  for(const [toggle,settings,field] of [['limit-machines','machine-limit-settings','max-machines'],['limit-row-width','row-width-settings','max-row-width']]) {
    const limited=$(toggle).checked;$(settings).hidden=!limited;$(field).disabled=!limited;$(field).required=limited;
  }
}
function dirty() {return state.artifact && !sameRequest(request(),state.artifact.factory.request);}
function populate(id,options,value) {
  const select=$(id);select.replaceChildren();
  for(const option of options){const name=typeof option==='string'?option:option.name;const e=element('option','',label(name));e.value=name;select.append(e);}
  if([...select.options].some(o=>o.value===value))select.value=value;
}
function recipeChanged(preferredMachine=$('machine').value) {
  const recipe=state.catalog.recipes.find(r=>r.name===$('recipe-select').value);
  if(!recipe)return;
  populate('machine',recipe.machines,chooseMachine(recipe,preferredMachine));
  text('recipe-id',recipe.name);text('product-name',recipeTitle(recipe));$('product-icon').src=iconURL(recipe.product);
  const ingredients=recipeIngredients(recipe),two=ingredients.length>=2;
  $('input-icon').src=iconURL(ingredients[0].item);text('input-label',ingredients.map(i=>label(i.item)).join(' + '));text('output-label',label(recipe.product));
  $('secondary-equipment').hidden=!two;$('secondary-inserter').disabled=!two;$('secondary-inserter').required=two;
  equipmentChanged();
}
function equipmentChanged() {
  for(const id of ['machine','belt','inserter','secondary-inserter','pole'])$(id+'-icon').src=iconURL($(id).value || 'long-handed-inserter');
  const find=(type,id)=>state.catalog[type].find(e=>e.name===$(id).value);
  const machine=find('machines','machine'),belt=find('belts','belt'),pole=find('poles','pole');
  text('machine-detail',machine ? `${machine.width} × ${machine.height} тайла · скорость ${number(machine.speed)}×` : '');
  text('belt-detail',belt ? `${number(belt.belt_rate)} предм./с · ${number(belt.belt_rate/2)} на полосу` : '');
  $('max-row-width').min=String((machine?.width || 2)+4);
  text('pole-detail',pole ? `Зона ${number(pole.supply_radius*2)} × ${number(pole.supply_radius*2)} · провод до ${number(pole.wire_reach)} тайла` : '');
  updateState();
}
function persist() {
  try{localStorage.setItem('factorium.form.v1',JSON.stringify(request()));}catch{/* Storage is optional. */}
}

async function initialize() {
  $('retry-button').disabled=true;showError('connection-error-text','');$('connection-error').hidden=true;
  try {
    const catalog=await api('/api/catalog');state.catalog=catalog;
    const recipes=[...catalog.recipes].sort((a,b)=>label(a.product).localeCompare(label(b.product),'ru'));
    if(!recipes.length)throw new Error('В каталоге нет поддерживаемых рецептов. Экспортируйте игровые данные и перезапустите сервер.');
    let saved;try{saved=JSON.parse(localStorage.getItem('factorium.form.v1'));}catch{/* Fresh form. */}
    const defaultRecipe=recipes.some(r=>r.name===saved?.recipe) ? saved.recipe : recipes.some(r=>r.name==='iron-gear-wheel') ? 'iron-gear-wheel' : recipes[0].name;
    populate('recipe-select',recipes.map(r=>({name:r.name})),defaultRecipe);
    populate('belt',catalog.belts,saved?.belt || 'transport-belt');
    populate('inserter',catalog.inserters,saved?.inserter || 'fast-inserter');
    populate('secondary-inserter',catalog.long_inserters || [],saved?.secondary_inserter || 'long-handed-inserter');
    populate('pole',catalog.poles,saved?.pole || 'small-electric-pole');
    populate('output-chest',catalog.chests || [],saved?.output_chest || 'steel-chest');restoreOptions(saved);
    if(saved?.rate>0 && Number.isFinite(saved.rate))$('rate').value=String(saved.rate);
    recipeChanged(saved?.machine || 'assembling-machine-2');
    text('recipe-count',`${recipes.length} доступно`);
    const badge=$('game-badge');badge.replaceChildren(element('span',`status-dot ${catalog.verification_available?'ready':''}`),element('span','',`Factorio ${catalog.game_version}`));
    badge.title=`${Object.keys(catalog.mods).length} активных модов · ${catalog.verification_hint}`;
    $('config-fields').disabled=false;
    await generate();
  } catch(error) {
    $('connection-error').hidden=false;showError('connection-error-text',error.message);
    text('game-badge','Нет соединения');
    const empty=$('canvas-empty');empty.hidden=false;empty.querySelector('strong').textContent='Не удалось загрузить каталог';empty.querySelector('span:last-child').textContent='Проверьте локальный сервер и повторите загрузку.';
  } finally {$('retry-button').disabled=false;}
}

function updateState() {
  const stale=dirty(),have=Boolean(state.artifact);
  const job=state.artifact?.verification;
  const report=job?.report,running=job?.status==='running';
  text('verification-title',stale?'Настройки изменены':running?'Проверяем фактический выпуск':report?(report.passed?'Цель подтверждена в игре':'Проверка не пройдена'):job?.status==='error'?'Испытание завершилось с ошибкой':'Проверьте выпуск в игре');
  text('verification-description',stale?'Результаты относятся к открытому чертежу. Постройте новый, чтобы проверить изменённые параметры.':state.catalog?.verification_available?'Factorio построит чертёж и измерит реальный выпуск при заданной подаче.':state.catalog?.verification_hint || '');
  $('dirty-notice').hidden=!stale;
  $('copy-button').disabled=!have || stale || state.generating;
  $('export-button').disabled=!have || stale || state.generating;
  $('verify-button').disabled=!have || stale || state.generating || !state.catalog?.verification_available || job?.status==='running';
  $('verify-button').title=!state.catalog?.verification_available ? state.catalog?.verification_hint || 'Загрузка каталога' : stale ? 'Сначала постройте чертёж с новыми параметрами' : '';
  $('generate-button').querySelector('span').textContent=state.generating?'Строим чертёж…':'Построить чертёж';
  $('rebuild-button').disabled=state.generating;
  $('canvas-busy').hidden=!state.generating;
  const status=state.generating ? {text:'Строим…',tone:'warning'} : stale ? {text:'Параметры изменены',tone:'warning'} : have ? verificationState(job) : {text:'Ожидание',tone:'neutral'};
  $('snapshot-badge').className=`badge ${status.tone}`;text('snapshot-badge',status.text);
  for(const button of document.querySelectorAll('[data-rate]'))button.classList.toggle('active',Number(button.dataset.rate)===Number($('rate').value));
}

async function generate(event) {
  event?.preventDefault();
  if(state.generating || !state.catalog)return;
  if(!$('factory-form').checkValidity()){
    if(activePanel!=='config-panel')openPanel('config-panel');
    $('factory-form').reportValidity();return;
  }
  const input=request();state.generating=true;showError('generation-error','');$('config-fields').disabled=true;updateState();
  try {
    const artifact=await api('/api/artifacts',{method:'POST',body:JSON.stringify(input)});
    if(event)closePanel(false);
    setArtifact(artifact);persist();await refreshHistory();
    if(event)$('viewport').focus({preventScroll:true});
  } catch(error){if(activePanel!=='config-panel')openPanel('config-panel');showError('generation-error',error.message);}
  finally{state.generating=false;$('config-fields').disabled=false;updateState();}
}

function setArtifact(artifact,restoreForm=false) {
  showError('generation-error','');
  stopPolling();state.artifact=artifact;state.selected=null;$('entity-inspector').hidden=true;
  const f=artifact.factory;
  if(restoreForm){populate('recipe-select',state.catalog.recipes.map(r=>({name:r.name})).sort((a,b)=>label(a.name).localeCompare(label(b.name),'ru')),f.request.recipe);$('rate').value=f.request.rate;recipeChanged(f.request.machine);for(const key of ['belt','inserter','pole'])$(key).value=f.request[key];if(f.request.secondary_inserter)$('secondary-inserter').value=f.request.secondary_inserter;restoreOptions(f.request);equipmentChanged();persist();}
  text('result-title',label(f.request.product));
  $('metric-output').replaceChildren(document.createTextNode(number(f.request.rate)),element('small','',' предм./с'));
  text('metric-output-note','Цель · выпуск не ниже заданного');
  text('metric-machines',number(f.machine_count,0));text('metric-machines-note',`Производственных рядов: ${number(f.rows.length,0)}`);
  const requirements=inputRequirements(f);
  $('metric-input').replaceChildren(document.createTextNode(number(requirements.reduce((sum,r)=>sum+r.supplied_per_second,0))),element('small','',' предм./с'));
  text('metric-input-note',requirements.length>=2?`${requirements.length} ингредиента · запас 5%`:'С запасом 5% к расчёту');
  text('metric-power',power(f.power.installed_power_watts));text('metric-power-note',`${f.power.pole_count} столбов · верхняя оценка нагрузки`);
  text('entity-count',`${f.document.blueprint.entities.length} сущностей`);
  text('canvas-caption',`${f.machine_count} машин · ${f.storage?.length ? `сундуков: ${f.storage.reduce((n,b)=>n+b.chests.length,0)}` : `выходных лент: ${outputPorts(f).length}`} · ${f.bus ? `входов шины: ${f.bus.lines.length} · ` : ''}${label(f.request.pole)}`);
  $('canvas-empty').hidden=true;
  drawFactory(f);renderPorts(f);renderVerification();updateState();
  if(artifact.verification.status==='running')startPolling(artifact.id);
}

function drawFactory(f) {
  const requirements=inputRequirements(f);
  document.querySelector('.legend-belt').style.backgroundColor=transportPalette(f.request.belt).rail;
  const scene=$('scene');scene.replaceChildren(transportSymbols(svg,f.request.belt),svg('rect',{x:-100000,y:-100000,width:200000,height:200000,fill:'url(#grid)'}));
  const powerGroup=svg('g',{'data-layer':'power'}),wireGroup=svg('g',{'data-layer':'wires'}),tunnelGroup=svg('g',{'data-layer':'tunnels','pointer-events':'none'}),bodyGroup=svg('g'),portsGroup=svg('g',{'data-layer':'ports'});
  const entities=f.document.blueprint.entities,byID=new Map(entities.map(e=>[e.entity_number,e]));
  const transport=transportLayout(entities,f.request.belt,f.bus);
  for(const {input,output} of transport.tunnels){
    const a=input.position,b=output.position,d=`M ${a.x} ${a.y} L ${b.x} ${b.y}`;
    tunnelGroup.append(svg('path',{d,fill:'none',stroke:'#0c1117','stroke-width':.15}));
    tunnelGroup.append(svg('path',{d,fill:'none',stroke:transportPalette(f.request.belt).arrow,'stroke-width':.045,'stroke-dasharray':'.14 .22','stroke-opacity':.4}));
  }
  let x0=Infinity,y0=Infinity,x1=-Infinity,y1=-Infinity;
  for(const e of entities) {
    const size=f.entity_sizes[e.name],rotated=[4,12].includes(e.direction);
    const [w,h]=rotated?[size[1],size[0]]:size,x=e.position.x,y=e.position.y;
    const isPole=e.name===f.request.pole;
    const radius=isPole ? Math.max(w/2,f.power.supply_area_distance) : w/2;
    x0=Math.min(x0,x-radius);x1=Math.max(x1,x+radius);y0=Math.min(y0,y-(isPole?radius:h/2));y1=Math.max(y1,y+(isPole?radius:h/2));
    if(isPole){const r=f.power.supply_area_distance;powerGroup.append(svg('rect',{x:x-r,y:y-r,width:2*r,height:2*r,fill:'#91d9ad','fill-opacity':.035,stroke:'#91d9ad','stroke-opacity':.23,'stroke-width':.045}));}
    const group=svg('g',{'data-entity':e.entity_number});group.append(svg('title',{},`${label(e.name)} · #${e.entity_number}${e.name===f.request.belt || e.name===f.bus?.splitter || e.name===f.bus?.underground?` · ${directionLabel(e.direction)}${e.type?` · ${e.type==='input'?'Вход под землю':'Выход на поверхность'}`:''}`:''}`));
    if(e.name===f.request.machine){
      group.append(svg('rect',{class:'entity-body',x:x-w/2+.06,y:y-h/2+.06,width:w-.12,height:h-.12,rx:.14,fill:'#24453e',stroke:'#70ad99','stroke-width':.06}));
      const size=Math.min(w,h)*.64;group.append(svg('image',{href:iconURL(e.name),x:x-size/2,y:y-size/2,width:size,height:size,'pointer-events':'none'}));
    } else if(e.name===f.request.belt){
      group.append(...transportTile(svg,e,transport.belts.get(e.entity_number)));
    } else if(e.name===f.bus?.splitter || e.name===f.bus?.underground){
      group.append(...transportDevice(svg,e,{splitter:f.bus.splitter,belt:f.request.belt,w,h,sprites:state.catalog.transport_sprites}));
    } else if(e.name==='constant-combinator'){
      const marker=f.markers?.find(m=>m.entity_number===e.entity_number);
      group.append(svg('rect',{class:'entity-body',x:x-.45,y:y-.45,width:.9,height:.9,rx:.08,fill:'#3c4e63',stroke:'#b6c9e2','stroke-width':.06}));
      group.append(svg('image',{href:iconURL(marker?.item || e.name),x:x-.35,y:y-.35,width:.7,height:.7,'pointer-events':'none'}));
    } else if(e.name===f.request.output_chest){
      group.append(svg('rect',{class:'entity-body',x:x-.44,y:y-.44,width:.88,height:.88,rx:.06,fill:'#5d4839',stroke:'#d6aa82','stroke-width':.06}));
      group.append(svg('image',{href:iconURL(e.name),x:x-.39,y:y-.39,width:.78,height:.78,'pointer-events':'none'}));
    } else if(e.name===f.request.inserter || requirements.some(r=>r.inserter===e.name)){
      const long=e.name!==f.request.inserter,reach=long?1.62:.62,color=long?'#e6a182':'#91b5d9';
      group.append(svg('rect',{class:'entity-body',x:x-.23,y:y-.23,width:.46,height:.46,rx:.1,fill:long?'#634037':'#304967',stroke:color,'stroke-width':.06}));
      group.append(svg('path',{d:`M ${x} ${y-reach} L ${x} ${y+reach}`,fill:'none',stroke:color,'stroke-width':.10,'pointer-events':'none'}));
      group.append(svg('path',{d:e.direction===8?`M ${x-.13} ${y-reach+.24} L ${x} ${y-reach} L ${x+.13} ${y-reach+.24}`:`M ${x-.13} ${y+reach-.24} L ${x} ${y+reach} L ${x+.13} ${y+reach-.24}`,fill:'none',stroke:color,'stroke-width':.07,'pointer-events':'none'}));
      group.append(svg('circle',{cx:x,cy:y,r:.11,fill:'#c1d7e9','pointer-events':'none'}));
    } else {
      group.append(svg('rect',{class:'entity-body',x:x-w/2+.12,y:y-h/2+.12,width:w-.24,height:h-.24,rx:.06,fill:'#776747',stroke:'#dac48b','stroke-width':.06}));
      group.append(svg('image',{href:iconURL(e.name),x:x-w*.38,y:y-h*.38,width:w*.76,height:h*.76,'pointer-events':'none'}));
    }
    bodyGroup.append(group);
  }
  for(const [a,,b] of f.document.blueprint.wires){const p=byID.get(a).position,q=byID.get(b).position;wireGroup.append(svg('path',{d:`M ${p.x} ${p.y} Q ${(p.x+q.x)/2} ${(p.y+q.y)/2+.3} ${q.x} ${q.y}`,fill:'none',stroke:'#dbb96d','stroke-opacity':.75,'stroke-width':.045}));}
  for(const [ports,isOutput] of [[inputBelts(f),false],[f.storage?.length ? f.storage.map(b=>({...byID.get(b.chests[0]),storage:true,direction:f.request.reverse_output?12:4})) : outputPorts(f),true]]) {
    ports.forEach((port,i)=>{
      const x=port.position.x,y=port.position.y,color=isOutput?'#edbd65':'#85d9ba';
      const north=port.direction===8;
      const west=isOutput && port.direction===12;
      portsGroup.append(svg('path',{d:north?`M ${x} ${y-2.2} v 1.7 m -.3 -.5 l .3 .5 .3 -.5`:west?`M ${x-.5} ${y} h -1.7 m .5 -.3 l -.5 .3 .5 .3`:isOutput?`M ${x+.5} ${y} h 1.7 m -.5 -.3 l .5 .3 -.5 .3`:`M ${x-2.2} ${y} h 1.7 m -.5 -.3 l .5 .3 -.5 .3`,fill:'none',stroke:color,'stroke-width':.09}));
      portsGroup.append(svg('text',{x:north?x:isOutput&&!west?x+2.5:x-2.5,y:north?y-2.7:y+.16,'text-anchor':north?'middle':isOutput&&!west?'start':'end',fill:color,'font-size':.52,'font-family':'Segoe UI, sans-serif'},`${port.storage?'Склад':isOutput?'Выход':'Вход'} ${i+1}`));
    });
  }
  for(const marker of f.markers || []) {
    const a=byID.get(marker.entity_number).position,b=byID.get(marker.target_entity).position;
    portsGroup.append(svg('path',{d:`M ${a.x} ${a.y} L ${b.x} ${b.y}`,fill:'none',stroke:'#a5bce1','stroke-width':.055,'stroke-dasharray':'.15 .15'}));
  }
  const p=f.power.connection_position;
  portsGroup.append(svg('circle',{cx:p.x,cy:p.y,r:.65,fill:'none',stroke:'#c9b5ec','stroke-width':.08,'stroke-dasharray':'.15 .12'}));
  scene.append(powerGroup,tunnelGroup,wireGroup,bodyGroup,portsGroup);
  const bounds={x:x0-8,y:y0-3,w:x1-x0+16,h:y1-y0+6};
  state.fitted=bounds;fit();
}

function applyView() {
  if(!state.view)return;const v=state.view;$('factory-canvas').setAttribute('viewBox',`${v.x} ${v.y} ${v.w} ${v.h}`);
  $('factory-canvas').classList.toggle('transport-small',$('viewport').clientWidth/v.w<12);
  text('zoom-level',`${Math.round((state.fitWidth || state.fitted.w)/v.w*100)}%`);
}
function fit(){
  if(!state.fitted)return;
  const b=state.fitted,viewport=$('viewport').getBoundingClientRect();
  let left=interfaceHidden?24:100,right=24,top=interfaceHidden?24:document.querySelector('.app-header').getBoundingClientRect().bottom+20,bottom=interfaceHidden?24:90;
  if(!interfaceHidden && viewport.width<=650){left=16;right=16;bottom=148;}
  if(activePanel && !interfaceHidden){const panel=$(activePanel).getBoundingClientRect();
    if(panel.width<viewport.width*.7){if(activePanel==='details-panel')right=viewport.width-panel.left+24;else left=panel.right+24;}
  }
  const availableW=Math.max(100,viewport.width-left-right),availableH=Math.max(100,viewport.height-top-bottom);
  const scale=Math.max(b.w/availableW,b.h/availableH),w=viewport.width*scale,h=viewport.height*scale;
  state.view={x:b.x+b.w/2-(left+availableW/2)*scale,y:b.y+b.h/2-(top+availableH/2)*scale,w,h};
  state.fitWidth=w;applyView();
}
function zoom(factor,at){
  if(!state.view)return;const v=state.view,ratio=(state.fitWidth || state.fitted.w)/v.w,next=ratio*factor;
  if(next<.3 || next>16)return;
  const anchor=at || {x:v.x+v.w/2,y:v.y+v.h/2};const w=v.w/factor,h=v.h/factor;
  state.view={x:anchor.x-(anchor.x-v.x)/factor,y:anchor.y-(anchor.y-v.y)/factor,w,h};applyView();
}
function screenPoint(event) {
  const p=$('factory-canvas').createSVGPoint();p.x=event.clientX;p.y=event.clientY;
  return p.matrixTransform($('factory-canvas').getScreenCTM().inverse());
}
function focusPoint(x,y) {
  closePanel(false);
  if(!state.fitted)return;const w=Math.min(22,state.fitted.w),h=w*$('viewport').clientHeight/$('viewport').clientWidth;
  state.view={x:x-w/2,y:y-h/2,w,h};applyView();$('viewport').focus({preventScroll:true});$('viewport').scrollIntoView({block:'nearest',behavior:'auto'});
}
function inspect(id) {
  const f=state.artifact?.factory,e=f?.document.blueprint.entities.find(e=>e.entity_number===id);if(!e)return;
  $('factory-canvas').querySelector('.selected')?.classList.remove('selected');
  $('factory-canvas').querySelector(`[data-entity="${id}"]`)?.classList.add('selected');
  text('inspector-kind',`СУЩНОСТЬ #${id}`);text('inspector-name',label(e.name));
  let detail=`x ${number(e.position.x)} · y ${number(e.position.y)}`;
  if(e.name===f.request.pole)detail+=`\nПитание: ${number(f.power.supply_area_distance*2)} × ${number(f.power.supply_area_distance*2)} · провод: ${number(f.power.max_wire_distance)}`;
  else if(e.name===f.request.machine)detail+=`\n${f.recipe}`;
  if(e.name===f.request.belt || e.name===f.bus?.splitter || e.name===f.bus?.underground)detail+=`\nНаправление: ${directionLabel(e.direction)}`;
  if(e.name===f.bus?.underground)detail+=e.type==='input'?'\nВход в подземный участок':'\nВыход на поверхность';
  const marker=f.markers?.find(m=>m.entity_number===id);if(marker)detail+=`\n${label(marker.item)} · ${number(marker.rate)}/с · вход #${marker.target_entity}`;
  text('inspector-detail',detail);$('entity-inspector').hidden=false;
}

function renderPorts(f) {
  const outputs=outputPorts(f),inputs=inputPorts(f),requirements=inputRequirements(f);
  $('ports-empty').hidden=true;$('ports-content').hidden=false;text('ports-count',String(inputBelts(f).length+outputs.length+1));
  text('ports-input-name',requirements.map(r=>label(r.item)).join(' + '));text('ports-output-name',label(f.request.product));text('theoretical-input',requirements.map(r=>`${label(r.item)}: ${number(r.per_second)}/с`).join(' · '));
  const body=$('ports-body');body.replaceChildren();
  const add=(name,item,rate,position,kind,hint)=>{
    const tr=element('tr'),nameCell=element('td');nameCell.append(element('span',`port-type ${kind}`),document.createTextNode(name));
    const itemCell=element('td','',item);if(hint)itemCell.append(element('span','port-hint',hint));
    const rateCell=element('td','',rate),pointCell=element('td'),button=element('button','coordinate-button',`${number(position.x)}; ${number(position.y)}`);button.type='button';button.setAttribute('aria-label',`Показать на схеме: ${name}`);button.addEventListener('click',()=>focusPoint(position.x,position.y));pointCell.append(button);tr.append(nameCell,itemCell,rateCell,pointCell);body.append(tr);
  };
  const inletIDs=inputBelts(f).map(port=>port.entity_number);
  inputs.forEach(port=>add(`Вход ${inletIDs.indexOf(port.entity_number)+1}`,label(port.item),`${number(port.rate)} /с`,port.position,'input',f.bus?`Шина ${port.direction===4?'с запада':'с севера'} · рядов: ${port.targets.length} · ${port.lanes==='both'?'обе полосы':port.lanes==='left'?'левая полоса':'правая полоса'}`:`Ряд ${port.row_index} · ${port.lanes==='both'?'обе полосы':port.lanes==='left'?'левая полоса':'правая полоса'}`));
  outputs.forEach((port,i)=>{
    const bank=f.storage?.find(b=>b.output_entity===port.entity_number),chest=bank && f.document.blueprint.entities.find(e=>e.entity_number===bank.chests[0]);
    const lanes=port.lanes==='both'?`Левая ${number(port.left_rate)}/с · правая ${number(port.right_rate)}/с`:`${port.lanes==='left'?'Левая':'Правая'} полоса`;
    add(bank?`Склад ${i+1}`:`Выход ${i+1}`,label(port.item),`${number(port.rate)} /с`,chest?.position || port.position,'output',bank?`${bank.chests.length} × ${label(f.request.output_chest)} · до заполнения` : `${lanes} · продолжение ${port.direction===12?'на запад':'на восток'}`);
  });
  add('Питание',`Столб #${f.power.connection_entity}`,`до ${power(f.power.installed_power_watts)}`,f.power.connection_position,'power','Подключите к внешней электросети');
}

function renderChart(rates,target) {
  const chart=$('rate-chart');chart.replaceChildren();if(!rates.length)return;
  const max=Math.max(target,...rates)*1.2 || 1,baseline=80,scale=58/max;
  chart.append(svg('path',{d:`M 8 ${baseline-target*scale} H 492`,stroke:'#e6b965','stroke-width':1,'stroke-dasharray':'4 4',fill:'none'}));
  rates.forEach((rate,i)=>{const x=35+i*121,h=rate*scale;chart.append(svg('rect',{x,y:baseline-h,width:66,height:h,rx:3,fill:rate>=target?'#78bca2':'#ce9c6c','fill-opacity':.5}));chart.append(svg('text',{x:x+33,y:baseline-h-7,'text-anchor':'middle',fill:'#d5e4dc','font-size':11,'font-family':'Consolas,monospace'},number(rate)));chart.append(svg('text',{x:x+33,y:101,'text-anchor':'middle',fill:'#8f9ead','font-size':10,'font-family':'Segoe UI,sans-serif'},`Окно ${i+1}`));});
  chart.setAttribute('aria-label',`Цель ${number(target)} в секунду. Измерения: ${rates.map(value=>number(value)).join(', ')}.`);
}

function renderLaneMeasurements(report,factory) {
  let section=$('lane-results');
  if(!section){section=element('details','simulation-settings');section.id='lane-results';$('verification-results').append(section);}
  section.replaceChildren();section.hidden=!report.row_outputs?.length;
  if(section.hidden)return;
  section.open=report.row_outputs.length<=4;
  section.append(element('summary','',`Выпуск по рядам ${report.row_measurement_location==='before-storage'?'до склада':''} · ${report.row_outputs.length}`));
  if(factory.bus)section.append(element('p','field-hint','Плановая доля ряда — ориентир. Делители перераспределяют поток; проверяются общий выпуск и работа каждого ряда.'));
  const scroll=element('div','table-scroll'),table=element('table'),head=element('thead'),header=element('tr');
  for(const name of ['Выход','Ряд / полоса','Цель, /с','Измерено, /с']){const th=element('th','',name);th.scope='col';header.append(th);}
  head.append(header);table.append(head);const body=element('tbody'),ports=outputPorts(factory);
  for(const row of report.row_outputs){const tr=element('tr');for(const value of [`Выход ${ports.findIndex(p=>p.entity_number===row.output_entity)+1}`,`${row.row_index} · ${row.lane==='left'?'левая':'правая'}`,number(row.target_per_second),number(row.measured_per_second)])tr.append(element('td','',value));body.append(tr);}
  table.append(body);scroll.append(table);section.append(scroll);
}

function renderInputMeasurements(report) {
  let section=$('ingredient-results');
  if(!section){section=element('details','simulation-settings');section.id='ingredient-results';$('verification-results').append(section);}
  section.replaceChildren();section.hidden=!report.input_deliveries?.length;
  if(section.hidden)return;
  section.open=report.input_deliveries.length<=4;section.append(element('summary','',report.input_measurement_location==='machines'?'Подача на шину и в машины':`Подача ингредиентов на входы · ${report.input_deliveries.length}`));
  const scroll=element('div','table-scroll'),table=element('table'),head=element('thead'),header=element('tr');
  for(const name of ['Предмет','Подключение / ряд','Подача, /с','Измерено, /с']){const th=element('th','',name);th.scope='col';header.append(th);}
  head.append(header);table.append(head);const body=element('tbody');
  for(const [i,input] of [...(report.external_input_deliveries || []),...report.input_deliveries].entries()){const tr=element('tr');for(const value of [label(input.item),input.row_index?`Ряд ${input.row_index}`:`Вход ${i+1}`,number(input.supplied_per_second),number(input.measured_per_second)])tr.append(element('td','',value));body.append(tr);}
  table.append(body);scroll.append(table);section.append(scroll);
}

function renderStorageMeasurements(report) {
  let section=$('storage-results');
  if(!section){section=element('details','simulation-settings');section.id='storage-results';$('verification-results').append(section);}
  section.replaceChildren();section.hidden=!report.storage_deliveries?.length;if(section.hidden)return;
  section.append(element('summary','',`Содержимое сундуков · ${report.storage_deliveries.length}`));
  const scroll=element('div','table-scroll'),table=element('table'),head=element('thead'),header=element('tr');
  for(const name of ['Сундук','Добавлено за измерение','Всего накоплено']){const th=element('th','',name);th.scope='col';header.append(th);}
  head.append(header);table.append(head);const body=element('tbody');
  for(const chest of report.storage_deliveries){const tr=element('tr');for(const value of [`#${chest.chest_entity}`,number(chest.added_items,0),number(chest.stored_items,0)])tr.append(element('td','',value));body.append(tr);}
  table.append(body);scroll.append(table);section.append(scroll);
}

function renderVerification() {
  const artifact=state.artifact,job=artifact?.verification,running=job?.status==='running',report=job?.report;
  $('verification-running').hidden=!running;$('verification-results').hidden=!report;
  $('warmup').disabled=running;$('measure').disabled=running;
  $('verification-dot').className=`tab-status-dot ${running?'running':report?(report.passed?'success':'error'):job?.status==='error'?'error':''}`;
  $('verify-button').querySelector('span').textContent=running?'Проверяем…':report || job?.status==='error'?'Проверить ещё раз':'Проверить в Factorio';
  showError('verification-errors',job?.error || report?.errors?.join('\n') || '');
  if(report){text('measured-rate',`${number(report.measured_output_per_second)} /с`);text('measured-target',`Цель: ≥ ${number(artifact.factory.request.rate)} предм./с`);text('network-count',number(report.network_count,0));text('powered-count',report.unpowered_entities.length?`${report.unpowered_entities.length} сущностей без питания`:'Все потребители запитаны');text('delivered-count',number(report.delivered_items,0));text('measurement-note',`${report.output_measurement_location==='chests'?'Добавлено в сундуки':'На выходных лентах'} · ${number(report.measurement_ticks/60,0)} игровых секунд`);renderChart(report.window_rates,artifact.factory.request.rate);}
  if(report)renderLaneMeasurements(report,artifact.factory);
  if(report){renderInputMeasurements(report);renderStorageMeasurements(report);}
  clearInterval(state.clock);state.clock=null;
  if(running){const update=()=>text('verification-elapsed',`${Math.max(0,Math.floor((Date.now()-job.started_at)/1000))} с`);update();state.clock=setInterval(update,1000);}
  updateState();
}

function stopPolling(){clearTimeout(state.poll);clearInterval(state.clock);state.poll=null;state.clock=null;}
function startPolling(id) {
  clearTimeout(state.poll);
  const poll=async()=>{
    if(state.artifact?.id!==id)return;
    try {
      const job=await api(`/api/artifacts/${id}/verification`);
      if(state.artifact?.id!==id)return;
      state.artifact.verification=job;renderVerification();
      if(job.status==='running')state.poll=setTimeout(poll,1100);
      else {await refreshHistory();if(job.report?.passed)toast(`Проверка пройдена: ${number(job.report.measured_output_per_second)} предм./с`);}
    } catch(error) {if(state.artifact?.id===id){showError('verification-errors',error.message);if(error.status===404){stopPolling();state.artifact.verification={status:'error',error:error.message};renderVerification();}else{state.poll=setTimeout(poll,3000);}}}
  };
  state.poll=setTimeout(poll,500);
}

async function verify() {
  const artifact=state.artifact;if(!artifact || dirty())return;
  if(!$('warmup').reportValidity() || !$('measure').reportValidity())return;
  $('verify-button').disabled=true;showError('verification-errors','');
  try {
    const job=await api(`/api/artifacts/${artifact.id}/verification`,{method:'POST',body:JSON.stringify({warmup_seconds:Number($('warmup').value),measurement_seconds:Number($('measure').value)})});
    if(state.artifact?.id!==artifact.id)return;
    artifact.verification=job;renderVerification();startPolling(artifact.id);await refreshHistory();
  }catch(error){showError('verification-errors',error.message);updateState();}
}

async function refreshHistory() {
  const {artifacts}=await api('/api/artifacts');state.history=artifacts;
  $('history').hidden=!artifacts.length;$('history-empty').hidden=Boolean(artifacts.length);const list=$('history-items');list.replaceChildren();
  for(const a of artifacts){const button=element('button',a.id===state.artifact?.id?'active':'');button.type='button';button.title=verificationState(a.verification).text;
    button.setAttribute('aria-pressed',String(a.id===state.artifact?.id));const img=element('img');img.src=iconURL(a.product);img.alt='';
    const description=element('span','',`${label(a.product)} · ${number(a.rate)}/с`);description.append(element('small','',verificationState(a.verification).text));button.append(img,description);
    button.addEventListener('click',async()=>{try{const artifact=await api(`/api/artifacts/${a.id}`);closePanel(false);setArtifact(artifact,true);await refreshHistory();$('viewport').focus({preventScroll:true});}catch(error){toast(error.message);}});list.append(button);}
}

async function blueprintString(artifact=state.artifact) {
  if(!artifact)throw new Error('Сначала постройте чертёж');
  if(!state.stringCache.has(artifact.id))state.stringCache.set(artifact.id,await api(`/api/artifacts/${artifact.id}/export/txt`));
  return state.stringCache.get(artifact.id);
}
async function openExport() {
  if(!state.artifact || dirty() || state.generating)return;
  const artifact=state.artifact,f=artifact.factory;
  text('export-description',`${label(f.request.product)} · цель ${number(f.request.rate)} предм./с · ${f.machine_count} машин · ${verificationState(artifact.verification).text.toLocaleLowerCase('ru-RU')}`);
  for(const format of ['txt','json','svg','factory','report'])$(`download-${format}`).href=`/api/artifacts/${artifact.id}/export/${format}`;
  $('download-report').hidden=!artifact.verification.report;
  $('blueprint-string').value='Загружаем строку…';$('dialog-copy-button').disabled=true;$('export-dialog').showModal();
  try{$('blueprint-string').value=await blueprintString(artifact);$('dialog-copy-button').disabled=false;}catch(error){$('blueprint-string').value=error.message;}
}
async function copyBlueprint(fromDialog=false) {
  if(!state.artifact || dirty() || state.generating)return;
  try {const value=await blueprintString();await navigator.clipboard.writeText(value);toast('Строка блупринта скопирована');}
  catch {if(!fromDialog)await openExport();$('blueprint-string').focus();$('blueprint-string').select();toast('Выделили строку. Нажмите Ctrl+C, чтобы скопировать.');}
}

function activateTab(name,focus=false) {
  for(const id of ['ports','verification']){const active=id===name;$(id+'-tab').setAttribute('aria-selected',String(active));$(id+'-tab').tabIndex=active?0:-1;$(id+'-panel').hidden=!active;}
  if(focus)$(name+'-tab').focus();
}

$('factory-form').addEventListener('submit',generate);
$('factory-form').addEventListener('input',()=>{optionsChanged();updateState();persist();});
$('recipe-select').addEventListener('change',()=>{recipeChanged();persist();});
for(const id of ['machine','belt','inserter','secondary-inserter','pole'])$(id).addEventListener('change',()=>{equipmentChanged();persist();});
for(const button of document.querySelectorAll('[data-rate]'))button.addEventListener('click',()=>{$('rate').value=button.dataset.rate;updateState();persist();});
for(const [id,layer] of [['show-power','power'],['show-wires','wires'],['show-ports','ports'],['show-tunnels','tunnels']])$(id).addEventListener('change',()=>$('viewport').classList.toggle(`hide-${layer}`,!$(id).checked));
$('zoom-in').addEventListener('click',()=>zoom(1.3));$('zoom-out').addEventListener('click',()=>zoom(1/1.3));$('zoom-fit').addEventListener('click',fit);
$('factory-canvas').addEventListener('wheel',event=>{if(!state.view)return;event.preventDefault();zoom(event.deltaY<0?1.12:1/1.12,screenPoint(event));},{passive:false});
$('factory-canvas').addEventListener('pointerdown',event=>{if(!state.view || event.button!==0)return;const p=screenPoint(event);state.drag={id:event.pointerId,start:p,view:{...state.view},clientX:event.clientX,clientY:event.clientY,target:event.target.closest('[data-entity]')?.dataset.entity};$('factory-canvas').setPointerCapture(event.pointerId);});
$('factory-canvas').addEventListener('pointermove',event=>{if(!state.drag || state.drag.id!==event.pointerId)return;const transform=$('factory-canvas').getScreenCTM(),dx=(event.clientX-state.drag.clientX)/transform.a,dy=(event.clientY-state.drag.clientY)/transform.d;state.view={...state.drag.view,x:state.drag.view.x-dx,y:state.drag.view.y-dy};applyView();});
$('factory-canvas').addEventListener('pointerup',event=>{if(!state.drag)return;const d=state.drag;state.drag=null;if(Math.hypot(event.clientX-d.clientX,event.clientY-d.clientY)<4 && d.target)inspect(Number(d.target));});
$('factory-canvas').addEventListener('pointercancel',()=>state.drag=null);
$('viewport').addEventListener('keydown',event=>{if(!state.view)return;const v=state.view,step=v.w*.07;const keys={ArrowLeft:[-step,0],ArrowRight:[step,0],ArrowUp:[0,-step],ArrowDown:[0,step]};if(keys[event.key]){event.preventDefault();const [x,y]=keys[event.key];state.view={...v,x:v.x+x,y:v.y+y};applyView();}else if(['+','=','-','0'].includes(event.key)){event.preventDefault();if(event.key==='0')fit();else zoom(event.key==='-'?1/1.3:1.3);}});
$('close-inspector').addEventListener('click',()=>{$('entity-inspector').hidden=true;$('factory-canvas').querySelector('.selected')?.classList.remove('selected');});
$('verify-button').addEventListener('click',verify);$('copy-button').addEventListener('click',()=>copyBlueprint());$('export-button').addEventListener('click',openExport);$('dialog-copy-button').addEventListener('click',()=>copyBlueprint(true));
$('help-button').addEventListener('click',()=>$('help-dialog').showModal());
for(const button of document.querySelectorAll('[data-close-dialog]'))button.addEventListener('click',()=>button.closest('dialog').close());
for(const dialog of document.querySelectorAll('dialog'))dialog.addEventListener('click',event=>{if(event.target!==dialog)return;const r=dialog.getBoundingClientRect();if(event.clientX<r.left || event.clientX>r.right || event.clientY<r.top || event.clientY>r.bottom)dialog.close();});
for(const name of ['ports','verification'])$(name+'-tab').addEventListener('click',()=>activateTab(name));
document.querySelector('.tabs').addEventListener('keydown',event=>{if(['ArrowLeft','ArrowRight','Home','End'].includes(event.key)){event.preventDefault();const current=$('ports-tab').getAttribute('aria-selected')==='true';activateTab(event.key==='Home'?'ports':event.key==='End'?'verification':current?'verification':'ports',true);}});
for(const id of ['warmup','measure'])$(id).addEventListener('input',()=>text('simulation-summary',`${$('warmup').value} с + ${$('measure').value} с`));
$('retry-button').addEventListener('click',initialize);
for(const button of document.querySelectorAll('[data-panel]'))button.addEventListener('click',()=>openPanel(button.dataset.panel,button));
for(const button of document.querySelectorAll('[data-close-panel]'))button.addEventListener('click',()=>closePanel());
$('toggle-interface').addEventListener('click',toggleInterface);
$('choose-product').addEventListener('click',openRecipePicker);
$('rebuild-button').addEventListener('click',generate);
$('recipe-search').addEventListener('input',()=>{recipeFilter='all';renderRecipePicker();});
$('recipe-search').addEventListener('keydown',event=>{
  const slots=$('recipe-results').querySelectorAll('.item-slot');
  if(event.key==='ArrowDown' && slots.length){event.preventDefault();slots[0].focus();}
  if(event.key==='Enter' && slots.length===1){event.preventDefault();slots[0].click();}
});
$('recipe-dialog').addEventListener('close',()=>$('choose-product').focus({preventScroll:true}));
$('reset-recipe-search').addEventListener('click',()=>{recipeFilter='all';$('recipe-search').value='';renderRecipePicker();$('recipe-search').focus();});
$('recipe-results').addEventListener('keydown',event=>{
  const current=event.target.closest('.item-slot');if(!current)return;
  const slots=[...$('recipe-results').querySelectorAll('.item-slot')],index=slots.indexOf(current);
  let next;
  if(event.key==='ArrowRight')next=slots[index+1];
  else if(event.key==='ArrowLeft')next=slots[index-1];
  else if(event.key==='Home')next=slots[0];
  else if(event.key==='End')next=slots.at(-1);
  else if(['ArrowUp','ArrowDown'].includes(event.key)){
    const rect=current.getBoundingClientRect(),direction=event.key==='ArrowDown'?1:-1;
    next=slots.filter(slot=>(slot.getBoundingClientRect().top-rect.top)*direction>4).sort((a,b)=>{
      const score=slot=>{const r=slot.getBoundingClientRect();return Math.abs(r.top-rect.top)*10+Math.abs(r.left-rect.left);};return score(a)-score(b);
    })[0];
  }else return;
  event.preventDefault();if(next){current.tabIndex=-1;next.tabIndex=0;next.focus();}
});
document.addEventListener('keydown',event=>{
  const dialog=document.querySelector('dialog[open]'),editing=event.target.closest('input,select,textarea,[contenteditable="true"]');
  if(dialog){if(dialog.id==='recipe-dialog' && (event.ctrlKey || event.metaKey) && event.code==='KeyF'){event.preventDefault();$('recipe-search').focus();}return;}
  if(event.key==='Escape'){closePanel();return;}
  if(editing || event.ctrlKey || event.metaKey || event.altKey || event.repeat)return;
  if(event.code==='KeyH'){event.preventDefault();toggleInterface();}
  if(event.code==='KeyE'){event.preventDefault();openRecipePicker();}
  if(event.code==='KeyP'){event.preventDefault();openPanel('config-panel');}
  if(event.code==='KeyI'){event.preventDefault();openPanel('details-panel');}
});
new ResizeObserver(()=>fit()).observe($('viewport'));
new ResizeObserver(()=>{
  const bottom=document.querySelector('.app-header').getBoundingClientRect().bottom;
  document.documentElement.style.setProperty('--overlay-top',`${Math.ceil(bottom+16)}px`);fit();
}).observe(document.querySelector('.app-header'));
window.addEventListener('pagehide',stopPolling);
initialize();
