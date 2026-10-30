// Directions use Factorio's 16-position compass; generated belts are cardinal.
const vectors = {0:[0,-1],4:[1,0],8:[0,1],12:[-1,0]};
const at = (x,y) => `${x},${y}`;
export const directionLabel = direction => ({0:'на север ↑',4:'на восток →',8:'на юг ↓',12:'на запад ←'}[direction ?? 0] || '');

export function transportLayout(entities,belt,bus) {
  const emitters=new Map(),belts=new Map(),tunnels=[];
  const emit=(x,y,direction)=>emitters.set(at(x,y),direction ?? 0);
  for(const e of entities) {
    const {x,y}=e.position,d=e.direction ?? 0;
    if(e.name===belt || (e.name===bus?.underground && e.type==='output'))emit(x,y,d);
    if(e.name===bus?.splitter){
      const [dx,dy]=vectors[d];
      for(const offset of [-.5,.5])emit(x-dy*offset,y+dx*offset,d);
    }
  }
  for(const e of entities) {
    const d=e.direction ?? 0,{x,y}=e.position;
    if(e.name===belt){
      const rear=(d+8)%16,sides=[(d+4)%16,(d+12)%16];
      const feedsFrom=side=>{const [dx,dy]=vectors[side];return emitters.get(at(x+dx,y+dy))===(side+8)%16;};
      const sideInputs=sides.filter(feedsFrom);
      // A rear feed (or two opposing side feeds) keeps a belt straight.
      const entry=!feedsFrom(rear) && sideInputs.length===1?sideInputs[0]:rear;
      const turn=entry!==rear,axis=d===0 || d===8?y:x;
      belts.set(e.entity_number,{direction:d,entry,turn,arrow:!turn && (Math.floor(axis)%4===0 || !feedsFrom(rear))});
    }
    if(e.name===bus?.underground && e.type==='input'){
      const [dx,dy]=vectors[d];
      const exit=entities.filter(other=>other.name===e.name && other.type==='output' && (other.direction ?? 0)===d)
        .map(other=>({entity:other,distance:(other.position.x-x)*dx+(other.position.y-y)*dy}))
        .filter(({entity:p,distance})=>distance>0 && (p.position.x-x)*dy===(p.position.y-y)*dx)
        .sort((a,b)=>a.distance-b.distance)[0]?.entity;
      if(exit)tunnels.push({input:e,output:exit});
    }
  }
  return {belts,tunnels};
}

export function transportPalette(name) {
  const arrow='#929d9e';
  if(name.startsWith('turbo-'))return {rail:'#536b60',arrow};
  if(name.startsWith('express-'))return {rail:'#526875',arrow};
  if(name.startsWith('fast-'))return {rail:'#765f5b',arrow};
  return {rail:'#716951',arrow};
}

// Reusable symbols keep large blueprints cheap: each tile adds one <use>.
export function transportSymbols(svg,name) {
  const defs=svg('defs'),palette=transportPalette(name);
  const path=(d,stroke,width,extra={})=>svg('path',{d,fill:'none',stroke,'stroke-width':width,...extra});
  for(const [id,d] of [['belt-straight','M -.5 0 H .5'],['belt-turn','M 0 -.5 A .5 .5 0 0 0 .5 0']]){
    const symbol=svg('symbol',{id,viewBox:'-.5 -.5 1 1',overflow:'visible'});
    symbol.append(path(d,'#111820',.94),path(d,palette.rail,.84),path(d,'#252f35',.76),path(d,'#3a454b',.025));
    // Only the tile boundaries, rather than repeated tread marks. Corners use
    // the same seams at their two connections as straight tiles.
    symbol.append(path(id==='belt-straight'?'M -.5 -.42 V .42 M .5 -.42 V .42':'M -.42 -.5 H .42 M .5 -.42 V .42','#526068',.025,{class:'belt-tile-seam','stroke-opacity':.65}));
    defs.append(symbol);
  }
  const arrow=svg('symbol',{id:'belt-arrow',viewBox:'-.5 -.5 1 1'});
  arrow.append(path('M -.12 -.14 L .06 0 L -.12 .14',palette.arrow,.06,{'stroke-linecap':'round','stroke-linejoin':'round'}));
  defs.append(arrow);return defs;
}

// Local space of every transport tile: +x is the flow, one tile is one unit.
// Belts and fallback device drawings share it so their lanes always line up.
export const transportRotation = direction => (direction ?? 0) * 22.5 - 90;

export function transportTile(svg,e,shape) {
  const d=e.direction ?? 0;
  const group=svg('g',{transform:`translate(${e.position.x} ${e.position.y}) rotate(${transportRotation(d)})`,'pointer-events':'none'});
  const id=shape.turn?'belt-turn':'belt-straight',mirror=shape.turn && shape.entry===(d+4)%16;
  group.append(svg('use',{href:`#${id}`,x:-.5,y:-.5,width:1,height:1,transform:mirror?'scale(1 -1)':'scale(1 1)'}));
  if(shape.arrow)group.append(svg('use',{href:'#belt-arrow',class:'belt-direction',x:-.5,y:-.5,width:1,height:1}));
  const hitbox=svg('rect',{class:'entity-body transport-hitbox',x:e.position.x-.48,y:e.position.y-.48,width:.96,height:.96,rx:.05,fill:'transparent',stroke:'transparent','stroke-width':.045});
  return [group,hitbox];
}

function chevron(svg,x,y,color,scale=1) {
  return svg('path',{class:'belt-direction',d:`M ${x-.07*scale} ${y-.12*scale} L ${x+.06*scale} ${y} L ${x-.07*scale} ${y+.12*scale}`,
    fill:'none',stroke:color,'stroke-width':.055,'stroke-linecap':'round','stroke-linejoin':'round','pointer-events':'none'});
}

// Static first frames of vanilla game sprites. Each direction has its own art;
// rotating inventory icons also rotates their perspective and gives wrong lanes.
// Sprite pixels are twice game pixels: 64 sprite pixels equal one tile.
export function transportSpriteFrames(e,available=[]) {
  const direction=(e.direction ?? 0)/4,frames=[];
  const frame=(asset,sx,sy,width,height,columns,rows,shiftX=0,shiftY=0)=>({asset,sx,sy,width,height,columns,rows,x:shiftX/32-width/128,y:shiftY/32-height/128});
  if(e.name.endsWith('underground-belt')){
    // The output sheet faces the opening: its direction is opposite the flow.
    const column=e.type==='output'?(direction+2)%4:direction;
    frames.push(frame(`${e.name}-structure`,column*192,e.type==='input'?192:0,192,192,4,4));
  } else if(e.name.endsWith('splitter')){
    const turbo=e.name==='turbo-splitter';
    const [facing,width,height,shiftX,shiftY]=(turbo?[
      ['north',158,66,7.5,1],['east',86,84,5,13],['south',164,64,4,.5],['west',90,84,6,13]
    ]:[
      ['north',160,70,7,0],['east',90,84,4,13],['south',164,64,4,0],['west',90,86,6,12]
    ])[direction];
    frames.push(frame(`${e.name}-${facing}`,0,0,width,height,8,4,shiftX,shiftY));
    if(direction===1 || direction===3){
      const height=direction===1?(turbo?102:104):96,shiftY=direction===1?(turbo?-19.5:-20):(turbo?-17.5:-18);
      frames.push(frame(`${e.name}-${facing}-top_patch`,0,0,90,height,8,4,turbo&&direction===1?4:shiftX,shiftY));
    }
  }
  return frames.length && frames.every(f=>available.includes(f.asset))?frames:[];
}

export function transportDevice(svg,e,{splitter,belt,w,h,sprites=[]}) {
  const palette=transportPalette(belt),metal='#7d8891';
  const group=svg('g',{transform:`translate(${e.position.x} ${e.position.y}) rotate(${transportRotation(e.direction)})`,'pointer-events':'none'});
  const lane=y=>svg('use',{href:'#belt-straight',x:-.5,y:y-.5,width:1,height:1});
  const hitbox=svg('rect',{class:'entity-body transport-hitbox',x:e.position.x-w/2+.02,y:e.position.y-h/2+.02,width:w-.04,height:h-.04,rx:.05,fill:'transparent',stroke:'transparent','stroke-width':.045});
  const frames=transportSpriteFrames(e,sprites);
  if(frames.length){
    if(e.name===splitter)group.append(lane(-.5),lane(.5));else group.append(lane(0));
    const art=svg('g',{class:'transport-device-sprite',transform:`translate(${e.position.x} ${e.position.y})`,'pointer-events':'none'});
    for(const f of frames){
      const crop=svg('svg',{x:f.x,y:f.y,width:f.width/64,height:f.height/64,viewBox:`${f.sx} ${f.sy} ${f.width} ${f.height}`,overflow:'hidden'});
      crop.append(svg('image',{href:`/api/transport-sprites/${f.asset}`,width:f.width*f.columns,height:f.height*f.rows}));
      art.append(crop);
    }
    return [group,art,hitbox];
  }
  // Schematics remain available without a local game install or for modded art.
  if(e.name===splitter){
    group.append(lane(-.5),lane(.5));
    // Guard between the lanes with the splitter's end caps.
    group.append(svg('path',{d:'M -.5 0 H .5',stroke:'#0d1319','stroke-width':.17,'stroke-linecap':'round'}));
    group.append(svg('path',{d:'M -.46 0 H .46',stroke:metal,'stroke-width':.055,'stroke-linecap':'round'}));
    group.append(svg('path',{d:'M -.46 -.94 v .16 M -.46 .94 v -.16 M .46 -.94 v .16 M .46 .94 v -.16',stroke:metal,'stroke-width':.09,'stroke-linecap':'round'}));
    for(const y of [-.5,.5])group.append(chevron(svg,.16,y,palette.arrow));
  } else {
    group.append(lane(0));
    // The hood covers the half where the belt dives under (input) or surfaces
    // (output); its opening faces the visible belt on the other side.
    const hood=svg('g',{});
    hood.append(svg('path',{d:'M 0 -.5 H .4 Q .5 -.5 .5 -.4 V .4 Q .5 .5 .4 .5 H 0 Q -.1 .3 -.1 0 Q -.1 -.3 0 -.5 Z',
      fill:'#141b22',stroke:'#616d78','stroke-width':.05}));
    hood.append(svg('path',{d:'M .04 -.4 H .38 M .04 .4 H .38',stroke:'#39434c','stroke-width':.045,'stroke-linecap':'round'}));
    hood.append(chevron(svg,-.3,0,palette.arrow,.9));
    hood.append(chevron(svg,-.46,0,palette.arrow,.6));
    if(e.type==='output')hood.setAttribute('transform','scale(-1 1)');
    group.append(hood);
  }
  return [group,hitbox];
}
