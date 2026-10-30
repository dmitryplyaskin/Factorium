// Static art uses the game's frame sizes, offsets and perspective unchanged.
const definition=(e,graphics)=>graphics?.entities?.[e.name];
const appearance=(e,graphics)=>definition(e,graphics)?.directions?.[e.direction ?? 0];

export function frameBounds(frame) {
  const width=frame.width*frame.scale/32,height=frame.height*frame.scale/32;
  return {x:frame.shift[0]-width/2,y:frame.shift[1]-height/2,width,height};
}

export function entitySymbols(svg,graphics,entities) {
  const defs=svg('defs'),used=new Set();
  for(const e of entities){
    const spec=definition(e,graphics),art=appearance(e,graphics);
    if(!art)continue;
    for(const frame of [...art.layers,...(spec.arm?[spec.arm.base,spec.arm.hand]:[])])used.add(frame.asset);
  }
  for(const id of used){
    const asset=graphics.assets[id];
    defs.append(svg('image',{id:`entity-art-${id}`,href:`/api/entity-sprites/${id}`,width:asset.width,height:asset.height}));
  }
  return defs;
}

function drawFrame(svg,frame,rect=frameBounds(frame)) {
  const crop=svg('svg',{...rect,viewBox:`${frame.x} ${frame.y} ${frame.width} ${frame.height}`,preserveAspectRatio:'none',overflow:'hidden'});
  crop.append(svg('use',{href:`#entity-art-${frame.asset}`}));return crop;
}

export function wirePoint(e,graphics) {
  const offset=appearance(e,graphics)?.wire || [0,0];
  return {x:e.position.x+offset[0],y:e.position.y+offset[1]};
}

export function inserterPose(e,graphics) {
  const arm=definition(e,graphics)?.arm;
  if(!arm)return null;
  const radians=(e.direction ?? 0)*Math.PI/8;
  const [dx,dy]=arm.drop;
  return {rotation:180+(e.direction ?? 0)*22.5,reach:Math.hypot(dx,dy),
    drop:{x:e.position.x+dx*Math.cos(radians)-dy*Math.sin(radians),y:e.position.y+dx*Math.sin(radians)+dy*Math.cos(radians)}};
}

export function entitySprite(svg,e,graphics,w,h) {
  const spec=definition(e,graphics),art=appearance(e,graphics);
  if(!art)return null;
  const translate=`translate(${e.position.x} ${e.position.y})`;
  const group=className=>svg('g',{class:className,transform:translate,'pointer-events':'none'});
  const body=group('game-entity-sprite'),shadow=group('game-entity-shadow'),arm=group('game-inserter-arm');
  body.setAttribute('pointer-events','visiblePainted');
  let left=-w/2,top=-h/2,right=w/2,bottom=h/2;
  for(const frame of art.layers){
    const rect=frameBounds(frame);
    (frame.shadow?shadow:body).append(drawFrame(svg,frame,rect));
    left=Math.min(left,rect.x);top=Math.min(top,rect.y);
    right=Math.max(right,rect.x+rect.width);bottom=Math.max(bottom,rect.y+rect.height);
  }
  const pose=inserterPose(e,graphics);
  if(pose){
    arm.setAttribute('data-entity',e.entity_number);
    arm.setAttribute('pointer-events','visiblePainted');
    const moving=svg('g',{transform:`rotate(${pose.rotation})`});
    const base=spec.arm.base,hand=spec.arm.hand;
    // The game stretches these two hand textures with the arm's extension.
    const upper=drawFrame(svg,base,{x:-base.width*base.scale/64,y:0,width:base.width*base.scale/32,height:pose.reach*.56});
    upper.setAttribute('transform','rotate(180)');
    moving.append(upper,drawFrame(svg,hand,{x:-hand.width*hand.scale/64,y:-pose.reach,width:hand.width*hand.scale/32,height:pose.reach*.58}));
    arm.append(moving);
    left=Math.min(left,pose.drop.x-e.position.x-.3);right=Math.max(right,pose.drop.x-e.position.x+.3);
    top=Math.min(top,pose.drop.y-e.position.y-.3);bottom=Math.max(bottom,pose.drop.y-e.position.y+.3);
  }
  const hitbox=svg('rect',{class:'entity-body game-entity-hitbox',x:e.position.x-w/2+.04,y:e.position.y-h/2+.04,width:w-.08,height:h-.08,rx:.06,fill:'transparent',stroke:'transparent','stroke-width':.045});
  return {body,shadow,arm,hitbox,bounds:{left:e.position.x+left,top:e.position.y+top,right:e.position.x+right,bottom:e.position.y+bottom}};
}
