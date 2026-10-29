const labels = {
  'iron-gear-wheel':'Железные шестерни','iron-plate':'Железные пластины','copper-plate':'Медные пластины',
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
  'small-electric-pole':'Малый столб','medium-electric-pole':'Средний столб','big-electric-pole':'Большая опора','substation':'Подстанция',
};
export function label(name) { return labels[name] || name; }
export function number(value, digits = 3) { return Number.isFinite(Number(value)) ? new Intl.NumberFormat('ru-RU', { maximumFractionDigits: digits }).format(Number(value)) : '—'; }
export function power(watts) { return watts >= 1e6 ? `${number(watts/1e6,2)} МВт` : watts >= 1000 ? `${number(watts/1000,1)} кВт` : `${number(watts,0)} Вт`; }
export function sameRequest(a,b) { return Boolean(a && b && ['product','recipe','machine','belt','inserter','pole'].every(key => (a[key] || null) === (b[key] || null)) && Number.isFinite(Number(a.rate)) && Number(a.rate) === Number(b.rate)); }
export function chooseMachine(recipe, current) {
  const allowed = recipe?.machines || [];
  return [current,'assembling-machine-2','electric-furnace','assembling-machine-1','assembling-machine-3',...allowed].find(name => allowed.includes(name)) || '';
}
export function verificationState(job) {
  if (job?.status === 'running') return { text:'Проверяем в игре', tone:'warning' };
  if (job?.status === 'complete') return job.report?.passed ? { text:'Проверено в игре', tone:'success' } : { text:'Проверка не пройдена', tone:'error' };
  if (job?.status === 'error') return { text:'Ошибка испытания', tone:'error' };
  return { text:'Схема проверена', tone:'neutral' };
}
