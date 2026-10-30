import { label, number, power, sameRequest, chooseMachine, verificationState, outputPorts, inputPorts, inputRequirements, recipeIngredients } from './model.js';

const $ = id => document.getElementById(id);
const svgNS = 'http://www.w3.org/2000/svg';
const state = { catalog:null, artifact:null, generating:false, poll:null, clock:null, history:[], view:null, fitted:null, drag:null, selected:null, stringCache:new Map() };
let toastTimer;

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
  return {product:recipe?.product || '',rate:Number($('rate').value),recipe:recipe?.name || null,machine:$('machine').value,belt:$('belt').value,inserter:$('inserter').value,secondary_inserter:recipeIngredients(recipe).length===2?$('secondary-inserter').value:null,pole:$('pole').value};
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
  text('recipe-id',recipe.name);$('product-icon').src=iconURL(recipe.product);
  const ingredients=recipeIngredients(recipe),two=ingredients.length===2;
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
    if(saved?.rate>0 && Number.isFinite(saved.rate))$('rate').value=String(saved.rate);
    recipeChanged(saved?.machine || 'assembling-machine-2');
    text('recipe-count',`${recipes.length} доступно`);
    const badge=$('game-badge');badge.replaceChildren(element('span',`status-dot ${catalog.verification_available?'ready':''}`),element('span','',`Factorio ${catalog.game_version}`));
    badge.title=`${Object.keys(catalog.mods).length} активных модов · ${catalog.verification_hint}`;
    $('config-fields').disabled=false;
    await generate();
  } catch(error) {
    $('connection-error').hidden=false;text('connection-error-text',error.message);
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
  $('canvas-busy').hidden=!state.generating;
  const status=state.generating ? {text:'Строим…',tone:'warning'} : stale ? {text:'Параметры изменены',tone:'warning'} : have ? verificationState(job) : {text:'Ожидание',tone:'neutral'};
  $('snapshot-badge').className=`badge ${status.tone}`;text('snapshot-badge',status.text);
  for(const button of document.querySelectorAll('[data-rate]'))button.classList.toggle('active',Number(button.dataset.rate)===Number($('rate').value));
}

async function generate(event) {
  event?.preventDefault();
  if(state.generating || !state.catalog || !$('factory-form').reportValidity())return;
  const input=request();state.generating=true;showError('generation-error','');$('config-fields').disabled=true;updateState();
  try {
    const artifact=await api('/api/artifacts',{method:'POST',body:JSON.stringify(input)});
    setArtifact(artifact);persist();await refreshHistory();
  } catch(error){showError('generation-error',error.message);}
  finally{state.generating=false;$('config-fields').disabled=false;updateState();}
}

function setArtifact(artifact,restoreForm=false) {
  showError('generation-error','');
  stopPolling();state.artifact=artifact;state.selected=null;$('entity-inspector').hidden=true;
  const f=artifact.factory;
  if(restoreForm){populate('recipe-select',state.catalog.recipes.map(r=>({name:r.name})).sort((a,b)=>label(a.name).localeCompare(label(b.name),'ru')),f.request.recipe);$('rate').value=f.request.rate;recipeChanged(f.request.machine);for(const key of ['belt','inserter','pole'])$(key).value=f.request[key];if(f.request.secondary_inserter)$('secondary-inserter').value=f.request.secondary_inserter;equipmentChanged();persist();}
  text('result-title',label(f.request.product));
  $('metric-output').replaceChildren(document.createTextNode(number(f.request.rate)),element('small','',' предм./с'));
  text('metric-output-note','Цель · выпуск не ниже заданного');
  text('metric-machines',number(f.machine_count,0));text('metric-machines-note',`Производственных рядов: ${number(f.rows.length,0)}`);
  const requirements=inputRequirements(f);
  $('metric-input').replaceChildren(document.createTextNode(number(requirements.reduce((sum,r)=>sum+r.supplied_per_second,0))),element('small','',' предм./с'));
  text('metric-input-note',requirements.length===2?'2 ингредиента · запас 5%':'С запасом 5% к расчёту');
  text('metric-power',power(f.power.installed_power_watts));text('metric-power-note',`${f.power.pole_count} столбов · верхняя оценка нагрузки`);
  text('entity-count',`${f.document.blueprint.entities.length} сущностей`);
  text('canvas-caption',`${f.machine_count} машин · выходных лент: ${outputPorts(f).length} · ${label(f.request.pole)} · ${number(f.power.supply_area_distance*2)} × ${number(f.power.supply_area_distance*2)} тайлов питания`);
  $('canvas-empty').hidden=true;
  drawFactory(f);renderPorts(f);renderVerification();updateState();
  if(artifact.verification.status==='running')startPolling(artifact.id);
}

function drawFactory(f) {
  const requirements=inputRequirements(f);
  const scene=$('scene');scene.replaceChildren(svg('rect',{x:-100000,y:-100000,width:200000,height:200000,fill:'url(#grid)'}));
  const powerGroup=svg('g',{'data-layer':'power'}),wireGroup=svg('g',{'data-layer':'wires'}),bodyGroup=svg('g'),portsGroup=svg('g',{'data-layer':'ports'});
  const entities=f.document.blueprint.entities,byID=new Map(entities.map(e=>[e.entity_number,e]));
  let x0=Infinity,y0=Infinity,x1=-Infinity,y1=-Infinity;
  for(const e of entities) {
    const [w,h]=f.entity_sizes[e.name],x=e.position.x,y=e.position.y;
    const isPole=e.name===f.request.pole;
    const radius=isPole ? Math.max(w/2,f.power.supply_area_distance) : w/2;
    x0=Math.min(x0,x-radius);x1=Math.max(x1,x+radius);y0=Math.min(y0,y-(isPole?radius:h/2));y1=Math.max(y1,y+(isPole?radius:h/2));
    if(isPole){const r=f.power.supply_area_distance;powerGroup.append(svg('rect',{x:x-r,y:y-r,width:2*r,height:2*r,fill:'#91d9ad','fill-opacity':.035,stroke:'#91d9ad','stroke-opacity':.23,'stroke-width':.045}));}
    const group=svg('g',{'data-entity':e.entity_number});group.append(svg('title',{},`${label(e.name)} · #${e.entity_number}`));
    if(e.name===f.request.machine){
      group.append(svg('rect',{class:'entity-body',x:x-w/2+.06,y:y-h/2+.06,width:w-.12,height:h-.12,rx:.14,fill:'#24453e',stroke:'#70ad99','stroke-width':.06}));
      const size=Math.min(w,h)*.64;group.append(svg('image',{href:iconURL(e.name),x:x-size/2,y:y-size/2,width:size,height:size,'pointer-events':'none'}));
    } else if(e.name===f.request.belt){
      group.append(svg('rect',{class:'entity-body',x:x-.47,y:y-.47,width:.94,height:.94,rx:.05,fill:'#7d683b',stroke:'#b39859','stroke-width':.035}));
      group.append(svg('path',{d:`M ${x-.27} ${y-.29} L ${x+.06} ${y-.29} L ${x-.08} ${y-.4} M ${x-.27} ${y+.21} L ${x+.12} ${y+.21} L ${x-.02} ${y+.1}`,fill:'none',stroke:'#dbbd73','stroke-width':.07,'pointer-events':'none'}));
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
  for(const [ports,isOutput] of [[inputPorts(f),false],[outputPorts(f),true]]) {
    ports.forEach((port,i)=>{
      const x=port.position.x,y=port.position.y,color=isOutput?'#edbd65':'#85d9ba';
      portsGroup.append(svg('path',{d:isOutput?`M ${x+.5} ${y} h 1.7 m -.5 -.3 l .5 .3 -.5 .3`:`M ${x-2.2} ${y} h 1.7 m -.5 -.3 l .5 .3 -.5 .3`,fill:'none',stroke:color,'stroke-width':.09}));
      portsGroup.append(svg('text',{x:isOutput?x+2.5:x-2.5,y:y+.16,'text-anchor':isOutput?'start':'end',fill:color,'font-size':.52,'font-family':'Segoe UI, sans-serif'},`${isOutput?'Выход':'Вход'} ${i+1}`));
    });
  }
  const p=f.power.connection_position;
  portsGroup.append(svg('circle',{cx:p.x,cy:p.y,r:.65,fill:'none',stroke:'#c9b5ec','stroke-width':.08,'stroke-dasharray':'.15 .12'}));
  scene.append(powerGroup,wireGroup,bodyGroup,portsGroup);
  const bounds={x:x0-8,y:y0-3,w:x1-x0+16,h:y1-y0+6};
  state.fitted=bounds;fit();
}

function applyView() {
  if(!state.view)return;const v=state.view;$('factory-canvas').setAttribute('viewBox',`${v.x} ${v.y} ${v.w} ${v.h}`);
  text('zoom-level',`${Math.round(state.fitted.w/v.w*100)}%`);
}
function fit(){if(!state.fitted)return;state.view={...state.fitted};applyView();}
function zoom(factor,at){
  if(!state.view)return;const v=state.view,ratio=state.fitted.w/v.w,next=ratio*factor;
  if(next<.3 || next>16)return;
  const anchor=at || {x:v.x+v.w/2,y:v.y+v.h/2};const w=v.w/factor,h=v.h/factor;
  state.view={x:anchor.x-(anchor.x-v.x)/factor,y:anchor.y-(anchor.y-v.y)/factor,w,h};applyView();
}
function screenPoint(event) {
  const p=$('factory-canvas').createSVGPoint();p.x=event.clientX;p.y=event.clientY;
  return p.matrixTransform($('factory-canvas').getScreenCTM().inverse());
}
function focusPoint(x,y) {
  if(!state.fitted)return;const w=Math.min(22,state.fitted.w),h=w*state.fitted.h/state.fitted.w;
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
  text('inspector-detail',detail);$('entity-inspector').hidden=false;
}

function renderPorts(f) {
  const outputs=outputPorts(f),inputs=inputPorts(f),requirements=inputRequirements(f);
  $('ports-empty').hidden=true;$('ports-content').hidden=false;text('ports-count',String(inputs.length+outputs.length+1));
  text('ports-input-name',requirements.map(r=>label(r.item)).join(' + '));text('ports-output-name',label(f.request.product));text('theoretical-input',requirements.map(r=>`${label(r.item)}: ${number(r.per_second)}/с`).join(' · '));
  const body=$('ports-body');body.replaceChildren();
  const add=(name,item,rate,position,kind,hint)=>{
    const tr=element('tr'),nameCell=element('td');nameCell.append(element('span',`port-type ${kind}`),document.createTextNode(name));
    const itemCell=element('td','',item);if(hint)itemCell.append(element('span','port-hint',hint));
    const rateCell=element('td','',rate),pointCell=element('td'),button=element('button','coordinate-button',`${number(position.x)}; ${number(position.y)}`);button.type='button';button.setAttribute('aria-label',`Показать на схеме: ${name}`);button.addEventListener('click',()=>focusPoint(position.x,position.y));pointCell.append(button);tr.append(nameCell,itemCell,rateCell,pointCell);body.append(tr);
  };
  inputs.forEach((port,i)=>add(`Вход ${i+1}`,label(port.item),`${number(port.rate)} /с`,port.position,'input',`Ряд ${port.row_index} · ${port.ingredient_index===1?'дальняя':'ближняя'} лента · обе полосы`));
  outputs.forEach((port,i)=>add(`Выход ${i+1}`,label(port.item),`${number(port.rate)} /с`,port.position,'output',port.lanes==='both'?`Левая ${number(port.left_rate)}/с · правая ${number(port.right_rate)}/с`:`${port.lanes==='left'?'Левая':'Правая'} полоса · продолжение на восток`));
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
  section.append(element('summary','',`Выпуск по рядам и полосам · ${report.row_outputs.length}`));
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
  section.open=report.input_deliveries.length<=4;section.append(element('summary','',`Подача ингредиентов · входов: ${report.input_deliveries.length}`));
  const scroll=element('div','table-scroll'),table=element('table'),head=element('thead'),header=element('tr');
  for(const name of ['Предмет','Ряд','Подача, /с','Измерено, /с']){const th=element('th','',name);th.scope='col';header.append(th);}
  head.append(header);table.append(head);const body=element('tbody');
  for(const input of report.input_deliveries){const tr=element('tr');for(const value of [label(input.item),input.row_index,number(input.supplied_per_second),number(input.measured_per_second)])tr.append(element('td','',value));body.append(tr);}
  table.append(body);scroll.append(table);section.append(scroll);
}

function renderVerification() {
  const artifact=state.artifact,job=artifact?.verification,running=job?.status==='running',report=job?.report;
  $('verification-running').hidden=!running;$('verification-results').hidden=!report;
  $('warmup').disabled=running;$('measure').disabled=running;
  $('verification-dot').className=`tab-status-dot ${running?'running':report?(report.passed?'success':'error'):job?.status==='error'?'error':''}`;
  $('verify-button').querySelector('span').textContent=running?'Проверяем…':report || job?.status==='error'?'Проверить ещё раз':'Проверить в Factorio';
  showError('verification-errors',job?.error || report?.errors?.join('\n') || '');
  if(report){text('measured-rate',`${number(report.measured_output_per_second)} /с`);text('measured-target',`Цель: ≥ ${number(artifact.factory.request.rate)} предм./с`);text('network-count',number(report.network_count,0));text('powered-count',report.unpowered_entities.length?`${report.unpowered_entities.length} сущностей без питания`:'Все потребители запитаны');text('delivered-count',number(report.delivered_items,0));text('measurement-note',`За ${number(report.measurement_ticks/60,0)} игровых секунд`);renderChart(report.window_rates,artifact.factory.request.rate);}
  if(report)renderLaneMeasurements(report,artifact.factory);
  if(report)renderInputMeasurements(report);
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
  $('history').hidden=artifacts.length<2;const list=$('history-items');list.replaceChildren();
  for(const a of artifacts.slice(0,6)){const button=element('button',a.id===state.artifact?.id?'active':'',`${label(a.product)} · ${number(a.rate)}/с`);button.type='button';button.title=verificationState(a.verification).text;button.addEventListener('click',async()=>{try{setArtifact(await api(`/api/artifacts/${a.id}`),true);await refreshHistory();}catch(error){toast(error.message);}});list.append(button);}
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
$('factory-form').addEventListener('input',()=>{updateState();persist();});
$('recipe-select').addEventListener('change',()=>{recipeChanged();persist();});
for(const id of ['machine','belt','inserter','secondary-inserter','pole'])$(id).addEventListener('change',()=>{equipmentChanged();persist();});
for(const button of document.querySelectorAll('[data-rate]'))button.addEventListener('click',()=>{$('rate').value=button.dataset.rate;updateState();persist();});
for(const [id,layer] of [['show-power','power'],['show-wires','wires'],['show-ports','ports']])$(id).addEventListener('change',()=>$('viewport').classList.toggle(`hide-${layer}`,!$(id).checked));
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
window.addEventListener('pagehide',stopPolling);
initialize();
