const labels = {
  'accumulator':'Аккумулятор','arithmetic-combinator':'Арифметический комбинатор',
  'battery-equipment':'Персональный аккумулятор','battery-mk3-equipment':'Персональный аккумулятор MK3',
  'belt-immunity-equipment':'Защита от конвейеров','boiler':'Паровой котёл','burner-inserter':'Твердотопливный манипулятор',
  'construction-robot':'Строительный робот','decider-combinator':'Решающий комбинатор','display-panel':'Информационная панель',
  'distractor-capsule':'Отвлекающая капсула','efficiency-module':'Модуль эффективности',
  'energy-shield-equipment':'Энергетический щит','explosive-rocket':'Разрывная ракета',
  'explosive-uranium-cannon-shell':'Разрывной урановый снаряд','flamethrower':'Огнемёт','grenade':'Граната',
  'heat-pipe':'Тепловая труба','heavy-armor':'Тяжёлая броня','land-mine':'Мина','logistic-robot':'Логистический робот',
  'logistic-science-pack':'Зелёные исследовательские пакеты','modular-armor':'Модульная броня',
  'night-vision-equipment':'Прибор ночного видения','nuclear-fuel':'Ядерное топливо','offshore-pump':'Береговой насос',
  'pipe-to-ground':'Подземная труба','productivity-module':'Модуль продуктивности','quality-module':'Модуль качества',
  'rail-chain-signal':'Проходной светофор','rail-signal':'Железнодорожный светофор','rail-support':'Железнодорожная опора',
  'repair-pack':'Ремонтный комплект','rocket':'Ракета','selector-combinator':'Комбинатор выбора',
  'shotgun-shell':'Патроны для дробовика','space-platform-foundation':'Основание космической платформы',
  'speed-module':'Модуль скорости','steel-furnace':'Стальная печь','storage-tank':'Резервуар',
  'toolbelt-equipment':'Пояс инструментов','tree-seed':'Семя дерева','uranium-cannon-shell':'Урановый снаряд',
  'uranium-rounds-magazine':'Магазин с урановыми патронами','explosive-cannon-shell':'Разрывной снаряд',
  'constant-combinator':'Постоянный комбинатор','splitter':'Делитель','fast-splitter':'Быстрый делитель','underground-belt':'Подземный конвейер','fast-underground-belt':'Быстрый подземный конвейер',
  'iron-gear-wheel':'Железные шестерни','iron-plate':'Железные пластины','copper-plate':'Медные пластины',
  'advanced-circuit':'Красные микросхемы','plastic-bar':'Пластмасса',
  'electronic-circuit':'Зелёные микросхемы','automation-science-pack':'Красные исследовательские пакеты',
  'copper-cable':'Медные провода','iron-ore':'Железная руда','copper-ore':'Медная руда','steel-plate':'Сталь',
  'iron-stick':'Железные прутья','pipe':'Трубы','barrel':'Пустые бочки','stone':'Камень','stone-brick':'Каменные блоки',
  'stone-wall':'Каменные стены','stone-furnace':'Каменные печи','landfill':'Отсыпка','light-armor':'Лёгкая броня',
  'iron-chest':'Железные сундуки','steel-chest':'Стальные сундуки','wooden-chest':'Деревянные сундуки','wood':'Древесина',
  'firearm-magazine':'Магазины с патронами','hazard-concrete':'Бетон с разметкой','refined-hazard-concrete':'Железобетон с разметкой',
  'concrete':'Бетон','refined-concrete':'Железобетон','lithium':'Литий','lithium-plate':'Литиевые пластины',
  'nuclear-fuel-reprocessing':'Переработка ядерного топлива','uranium-238':'Уран-238','depleted-uranium-fuel-cell':'Отработанные топливные элементы',
  'assembling-machine-1':'Сборочный автомат 1','assembling-machine-2':'Сборочный автомат 2','assembling-machine-3':'Сборочный автомат 3',
  'electric-furnace':'Электрическая печь','electromagnetic-plant':'Электромагнитный завод','crusher':'Дробилка',
  'transport-belt':'Обычный конвейер','fast-transport-belt':'Быстрый конвейер','express-transport-belt':'Экспресс-конвейер','turbo-transport-belt':'Турбоконвейер',
  'inserter':'Обычный манипулятор','fast-inserter':'Быстрый манипулятор','bulk-inserter':'Пакетный манипулятор','stack-inserter':'Стаковый манипулятор',
  'long-handed-inserter':'Длинный манипулятор',
  'small-electric-pole':'Малый столб','medium-electric-pole':'Средний столб','big-electric-pole':'Большая опора','substation':'Подстанция',
};
export function label(name) { return labels[name] || name; }

export const itemGroups = {
  'logistics': { label:'Логистика', icon:'transport-belt' },
  'production': { label:'Производство', icon:'assembling-machine-2' },
  'intermediate-products': { label:'Компоненты', icon:'iron-gear-wheel' },
  'space': { label:'Космос', icon:'space-platform-foundation' },
  'combat': { label:'Вооружение', icon:'firearm-magazine' },
  'other': { label:'Прочее', icon:'wooden-chest' },
};
const subgroupLabels = {
  'raw-resource':'Сырьё', 'raw-material':'Материалы', 'intermediate-product':'Компоненты',
  'science-pack':'Исследовательские пакеты', 'fluid-recipes':'Жидкости', 'terrain':'Покрытия и отсыпка',
  'storage':'Хранение', 'belt':'Конвейеры', 'inserter':'Манипуляторы', 'pipe':'Трубы',
  'energy-pipe-distribution':'Распределение энергии', 'train-transport':'Железная дорога',
  'transport':'Транспорт', 'logistic-network':'Логистическая сеть', 'circuit-network':'Логическая сеть',
  'space-related':'Космические платформы', 'tool':'Инструменты', 'energy':'Энергетика',
  'extraction-machine':'Добыча', 'smelting-machine':'Плавка', 'production-machine':'Производственные машины',
  'module':'Модули', 'gun':'Оружие', 'ammo':'Боеприпасы', 'capsule':'Капсулы и гранаты',
  'armor':'Броня', 'equipment':'Снаряжение', 'defensive-structure':'Оборона',
  'military-equipment':'Боевое снаряжение', 'utility-equipment':'Вспомогательное снаряжение',
  'space-platform':'Космические платформы', 'nauvis-agriculture':'Растения Наувиса',
  'uranium-processing':'Переработка урана', 'aquilo-processes':'Процессы Аквило',
};
// Base-game and Space Age subgroup order; unfamiliar modded rows follow by name.
const subgroupOrder = [
  'storage','belt','inserter','energy-pipe-distribution','train-transport','transport','logistic-network','circuit-network','terrain',
  'tool','energy','extraction-machine','smelting-machine','agriculture','production-machine','environmental-protection','module',
  'fluid-recipes','raw-resource','raw-material','barrel','fill-barrel','empty-barrel','intermediate-product','intermediate-recipe',
  'uranium-processing','vulcanus-processes','fulgora-processes','agriculture-processes','agriculture-products','nauvis-agriculture','aquilo-processes','science-pack',
  'space-interactors','space-platform','space-rocket','space-related','space-environment','space-material','space-crushing','space-processing',
  'gun','ammo','capsule','armor','equipment','utility-equipment','military-equipment','defensive-structure','turret',
];
function subgroupRank(recipe) {const index=subgroupOrder.indexOf(recipe.subgroup);return index<0?subgroupOrder.length:index;}
export function groupInfo(group) { return itemGroups[group] || {label:group,icon:'wooden-chest'}; }
export function subgroupLabel(subgroup) { return subgroupLabels[subgroup] || subgroup; }
export function recipeGroup(recipe) { return recipe.group || 'other'; }
export function recipeTitle(recipe) {
  const product=label(recipe.product);
  return recipe.name===recipe.product ? product : `${product} · ${label(recipe.name)}`;
}
function searchText(value) { return String(value).toLocaleLowerCase('ru').replaceAll('ё','е').replaceAll('-',' '); }
export function filterRecipes(recipes,query='',group='all') {
  const words=searchText(query).trim().split(/\s+/).filter(Boolean);
  return recipes.filter(recipe=>{
    if(group!=='all' && recipeGroup(recipe)!==group)return false;
    const haystack=searchText(`${label(recipe.product)} ${label(recipe.name)} ${recipe.product} ${recipe.name}`);
    return words.every(word=>haystack.includes(word));
  }).sort((a,b)=>subgroupRank(a)-subgroupRank(b) || String(a.subgroup || '').localeCompare(String(b.subgroup || '')) || String(a.order || '').localeCompare(String(b.order || '')) || recipeTitle(a).localeCompare(recipeTitle(b),'ru'));
}
export function number(value, digits = 3) { return Number.isFinite(Number(value)) ? new Intl.NumberFormat('ru-RU', { maximumFractionDigits: digits }).format(Number(value)) : '—'; }
export function power(watts) { return watts >= 1e6 ? `${number(watts/1e6,2)} МВт` : watts >= 1000 ? `${number(watts/1000,1)} кВт` : `${number(watts,0)} Вт`; }
export function sameRequest(a,b) { return Boolean(a && b && ['product','recipe','machine','belt','inserter','secondary_inserter','pole','output_chest'].every(key => (a[key] || null) === (b[key] || null)) && ['common_input','reverse_output','input_markers'].every(key=>Boolean(a[key])===Boolean(b[key])) && ['max_machines_per_row','max_row_width'].every(key=>(a[key] ? Number(a[key]) : null)===(b[key] ? Number(b[key]) : null)) && Number.isFinite(Number(a.rate)) && Number(a.rate) === Number(b.rate)); }
export function chooseMachine(recipe, current) {
  const allowed = recipe?.machines || [];
  return [current,'assembling-machine-2','electric-furnace','assembling-machine-1','assembling-machine-3',...allowed].find(name => allowed.includes(name)) || '';
}
// A shared output is one physical port; rates in factory.rows are contributions
// assigned to its two lanes, not two copies of the combined boundary flow.
export function outputPorts(factory) {
  const ports = new Map();
  for(const [index,row] of factory.rows.entries()) {
    let port=ports.get(row.output.entity_number);
    if(!port){port={...row.output,rate:0,left_rate:0,right_rate:0,rows:[]};ports.set(port.entity_number,port);}
    port.rate+=row.output.rate;
    port[`${row.output.lanes}_rate`]+=row.output.rate;
    port.rows.push(index+1);
    if(port.lanes!==row.output.lanes)port.lanes='both';
  }
  return [...ports.values()];
}
export function recipeIngredients(recipe) {return recipe?.ingredients || (recipe ? [{item:recipe.input,amount:recipe.input_amount}] : []);}
export function inputRequirements(factory) {return factory.inputs?.length ? factory.inputs : [{item:factory.input_item,per_second:factory.input_per_second,supplied_per_second:factory.supplied_input_per_second,inserter:factory.request.inserter}];}
export function inputPorts(factory) {return factory.bus ? factory.bus.lines.map(line=>({...line.source,targets:line.targets})) : factory.rows.flatMap((row,i)=>[row.input,...(row.additional_inputs || [])].map((port,j)=>({...port,row_index:i+1,ingredient_index:j})));}
export function verificationState(job) {
  if (job?.status === 'running') return { text:'Проверяем в игре', tone:'warning' };
  if (job?.status === 'complete') return job.report?.passed ? { text:'Проверено в игре', tone:'success' } : { text:'Проверка не пройдена', tone:'error' };
  if (job?.status === 'error') return { text:'Ошибка испытания', tone:'error' };
  return { text:'Схема проверена', tone:'neutral' };
}

// Keep material ports separate while counting and drawing each physical inlet once.
export function inputBelts(factory) {
  const seen=new Set();
  return inputPorts(factory).filter(port=>{
    if(seen.has(port.entity_number))return false;
    seen.add(port.entity_number);return true;
  });
}
