import {frameBounds,inserterPose} from './entities.js';
import {beltSpriteLayers,transportSpriteFrames} from './transport.js';

export const CHUNK_SIZE=16;
export const CACHE_BYTES=64*1024*1024;
const compass={0:[0,-1],4:[1,0],8:[0,1],12:[-1,0]};
const straight={0:3,4:1,8:4,12:2};
const intersects=(a,b)=>a.x<b.x+b.w && a.x+a.w>b.x && a.y<b.y+b.h && a.y+a.h>b.y;
const contains=(r,p)=>p.x>=r.x && p.x<=r.x+r.w && p.y>=r.y && p.y<=r.y+r.h;
const range=(r,size)=>({x0:Math.floor(r.x/size),y0:Math.floor(r.y/size),x1:Math.max(Math.floor(r.x/size),Math.ceil((r.x+r.w)/size)-1),y1:Math.max(Math.floor(r.y/size),Math.ceil((r.y+r.h)/size)-1)});

// One grid serves both viewport culling and hit testing; queries never scan the
// full factory, and indices preserve the original painting order.
export class SpatialIndex {
  constructor(items,size=CHUNK_SIZE){
    this.items=items;this.size=size;this.buckets=new Map();
    items.forEach((item,index)=>{
      const r=range(item.bounds,size);
      for(let y=r.y0;y<=r.y1;y++)for(let x=r.x0;x<=r.x1;x++){
        const key=`${x},${y}`;
        if(!this.buckets.has(key))this.buckets.set(key,[]);
        this.buckets.get(key).push(index);
      }
    });
  }
  visible(rect){
    const r=range(rect,this.size),keys=[];
    for(let y=r.y0;y<=r.y1;y++)for(let x=r.x0;x<=r.x1;x++)if(this.buckets.has(`${x},${y}`))keys.push([x,y]);
    return keys;
  }
  query(rect){
    const indices=new Set();
    for(const [x,y] of this.visible(rect))for(const index of this.buckets.get(`${x},${y}`))if(intersects(this.items[index].bounds,rect))indices.add(index);
    return [...indices].sort((a,b)=>a-b).map(index=>this.items[index]);
  }
  hit(point){
    const bucket=this.buckets.get(`${Math.floor(point.x/this.size)},${Math.floor(point.y/this.size)}`) || [];
    for(let i=bucket.length-1;i>=0;i--){const item=this.items[bucket[i]];if(contains(item.bounds,point))return item;}
    return null;
  }
}

export class BudgetCache {
  constructor(limit=CACHE_BYTES){this.limit=limit;this.bytes=0;this.entries=new Map();}
  get(key){const entry=this.entries.get(key);if(entry){this.entries.delete(key);this.entries.set(key,entry);}return entry?.value;}
  set(key,value,bytes){
    const old=this.entries.get(key);if(old){this.bytes-=old.bytes;this.entries.delete(key);}
    if(bytes>this.limit)return;
    this.entries.set(key,{value,bytes});this.bytes+=bytes;
    while(this.bytes>this.limit){const first=this.entries.keys().next().value;this.bytes-=this.entries.get(first).bytes;this.entries.delete(first);}
  }
  clear(){this.entries.clear();this.bytes=0;}
}

export function chunkResolution(pixelsPerTile,visibleCount,budget=CACHE_BYTES){
  let scale=Math.min(64,Math.max(1,2**Math.ceil(Math.log2(pixelsPerTile))));
  while(scale>1 && visibleCount*(CHUNK_SIZE*scale+2)**2*4>budget*.75)scale/=2;
  return scale;
}

// Use viewport capacity, not the occupied cells currently under the camera:
// crossing a chunk boundary must never change the image resolution while panning.
export function viewportResolution(view,width,budget=CACHE_BYTES){
  const capacity=(Math.ceil(view.w/CHUNK_SIZE)+1)*(Math.ceil(view.h/CHUNK_SIZE)+1);
  return chunkResolution(width/view.w,capacity,budget);
}

export function cachedChunk(cache,x,y,scale){
  for(let candidate=scale;candidate>=1;candidate/=2){
    const tile=cache.get(`${candidate}:${x},${y}`);if(tile)return {tile,scale:candidate};
  }
  for(let candidate=scale*2;candidate<=64;candidate*=2){
    const tile=cache.get(`${candidate}:${x},${y}`);if(tile)return {tile,scale:candidate};
  }
  return null;
}

function transformedBounds(x,y,w,h,rotation,ox,oy){
  const a=rotation*Math.PI/180,c=Math.cos(a),s=Math.sin(a);
  const points=[[x,y],[x+w,y],[x,y+h],[x+w,y+h]].map(([px,py])=>[ox+px*c-py*s,oy+px*s+py*c]);
  const xs=points.map(p=>p[0]),ys=points.map(p=>p[1]);
  return {x:Math.min(...xs),y:Math.min(...ys),w:Math.max(...xs)-Math.min(...xs),h:Math.max(...ys)-Math.min(...ys)};
}
const union=rects=>({left:Math.min(...rects.map(r=>r.x)),top:Math.min(...rects.map(r=>r.y)),right:Math.max(...rects.map(r=>r.x+r.w)),bottom:Math.max(...rects.map(r=>r.y+r.h))});

// Compact draw commands replace thousands of SVG clipping viewports. The
// original frame geometry is shared with the SVG fallback renderer.
export function buildSpriteScene(factory,catalog,transport){
  const frames=new Map(),bounds=new Map(),floor=[],shadows=[],bodies=[],arms=[],floorHits=[],bodyHits=[],armHits=[];
  const graphics=catalog.entity_graphics,sprites=catalog.transport_sprites || [],belt=factory.request.belt;
  const frame=(url,sx,sy,sw,sh)=>{
    const key=`${url}:${sx},${sy},${sw},${sh}`;
    if(!frames.has(key))frames.set(key,{url,sx,sy,sw,sh});return key;
  };
  const add=(list,key,x,y,w,h,rotation=0,ox=0,oy=0)=>{
    const command={key,x,y,w,h,rotation,ox,oy,bounds:transformedBounds(x,y,w,h,rotation,ox,oy)};
    list.push(command);return command;
  };
  const beltFrame=index=>frame(`/api/transport-sprites/${belt}`,0,(index-1)*128,128,128);
  const beltAt=(index,x,y)=>add(floor,beltFrame(index),x-1,y-1,2,2);
  const isFloor=e=>[belt,factory.bus?.splitter,factory.bus?.underground].includes(e.name);
  const entities=[...factory.document.blueprint.entities].sort((a,b)=>Number(!isFloor(a))-Number(!isFloor(b)) || a.position.y-b.position.y || a.position.x-b.position.x);
  for(const e of entities){
    const d=e.direction ?? 0,{x,y}=e.position,size=factory.entity_sizes[e.name];
    const [w,h]=[4,12].includes(d)?[size[1],size[0]]:size;
    const footprint={x:x-w/2,y:y-h/2,w,h},own=[];
    if(e.name===belt && sprites.includes(belt)){
      for(const layer of beltSpriteLayers(transport.belts.get(e.entity_number)))own.push(beltAt(layer.index,x+layer.x,y+layer.y).bounds);
      // Subtle tile boundaries remain visible at a close zoom.
      floor.push({seam:true,bounds:footprint});floorHits.push({entity:e.entity_number,bounds:footprint});
    } else if(isFloor(e) && sprites.includes(belt)){
      const deviceFrames=transportSpriteFrames(e,sprites);
      if(!deviceFrames.length)continue;
      const [dx,dy]=compass[d];
      for(const offset of e.name===factory.bus?.splitter?[-.5,.5]:[0])own.push(beltAt(straight[d],x-dy*offset,y+dx*offset).bounds);
      for(const f of deviceFrames)own.push(add(floor,frame(`/api/transport-sprites/${f.asset}`,f.sx,f.sy,f.width,f.height),x+f.x,y+f.y,f.width/64,f.height/64).bounds);
      floorHits.push({entity:e.entity_number,bounds:footprint});
    } else {
      const spec=graphics?.entities?.[e.name],art=spec?.directions?.[d];
      if(!art)continue;
      const visible=[];
      for(const f of art.layers){
        const r=frameBounds(f),key=frame(`/api/entity-sprites/${f.asset}`,f.x,f.y,f.width,f.height);
        const command=add(f.shadow?shadows:bodies,key,x+r.x,y+r.y,r.width,r.height);
        own.push(command.bounds);if(!f.shadow)visible.push(command.bounds);
      }
      const body=union([footprint,...visible]);bodyHits.push({entity:e.entity_number,bounds:{x:body.left,y:body.top,w:body.right-body.left,h:body.bottom-body.top}});
      const pose=inserterPose(e,graphics);
      if(pose){
        const base=spec.arm.base,hand=spec.arm.hand;
        const baseCommand=add(arms,frame(`/api/entity-sprites/${base.asset}`,base.x,base.y,base.width,base.height),-base.width*base.scale/64,0,base.width*base.scale/32,pose.reach*.56,pose.rotation+180,x,y);
        const handCommand=add(arms,frame(`/api/entity-sprites/${hand.asset}`,hand.x,hand.y,hand.width,hand.height),-hand.width*hand.scale/64,-pose.reach,hand.width*hand.scale/32,pose.reach*.58,pose.rotation,x,y);
        own.push(baseCommand.bounds,handCommand.bounds);
        armHits.push({entity:e.entity_number,bounds:baseCommand.bounds},{entity:e.entity_number,bounds:handCommand.bounds});
      }
    }
    bounds.set(e.entity_number,{...union([footprint,...own]),footprint});
  }
  return {commands:[...floor,...shadows,...bodies,...arms],frames,bounds,hits:[...floorHits,...bodyHits,...armHits]};
}

const surface=(w,h)=>{
  const canvas=typeof OffscreenCanvas==='function'?new OffscreenCanvas(w,h):document.createElement('canvas');
  canvas.width=w;canvas.height=h;return canvas;
};

async function loadFrames(specs){
  const groups=new Map(),result=new Map();
  for(const [key,spec] of specs){if(!groups.has(spec.url))groups.set(spec.url,[]);groups.get(spec.url).push([key,spec]);}
  const jobs=[...groups],errors=[];let cursor=0;
  await Promise.all(Array.from({length:Math.min(4,jobs.length)},async()=>{
    while(cursor<jobs.length){
      const [url,frames]=jobs[cursor++];let source;
      try{
        const response=await fetch(url);if(!response.ok)throw new Error(`Sprite ${response.status}: ${url}`);
        source=await createImageBitmap(await response.blob());
        for(const [key,f] of frames)result.set(key,await createImageBitmap(source,f.sx,f.sy,f.sw,f.sh));
      }catch(error){errors.push(error);}finally{source?.close();}
    }
  }));
  return {frames:result,errors};
}

export class SpriteRenderer {
  constructor(factory,catalog,transport){
    this.canvas=document.createElement('canvas');this.context=this.canvas.getContext('2d');
    this.enabled=Boolean(this.context && typeof createImageBitmap==='function');
    this.bounds=new Map();if(!this.enabled)return;
    // A screen-sized HTML canvas avoids foreignObject's extra rasterization in
    // SVG user units, which quantizes sprite positions independently of overlays.
    this.node=this.canvas;this.node.dataset.renderer='cached-sprites';this.node.setAttribute('aria-hidden','true');
    this.canvas.className='sprite-canvas';
    const scene=buildSpriteScene(factory,catalog,transport);
    this.bounds=scene.bounds;this.index=new SpatialIndex(scene.commands);this.hits=new SpatialIndex(scene.hits);
    this.cache=new BudgetCache();this.frames=new Map();this.ready=false;this.destroyed=false;this.pending=0;
    this.node.dataset.sprites=String(scene.commands.filter(command=>command.key).length);
    loadFrames(scene.frames).then(result=>{
      if(this.destroyed){for(const image of result.frames.values())image.close();return;}
      this.frames=result.frames;this.ready=true;
      this.node.dataset.spriteErrors=String(result.errors.length);
      if(result.errors.length)console.warn('Some blueprint sprites could not be loaded',result.errors);
      this.schedule();
    });
  }
  destroy(){
    this.destroyed=true;cancelAnimationFrame(this.pending);
    this.node?.remove();
    for(const frame of this.frames?.values() || [])frame.close();
    this.frames?.clear();this.cache?.clear();
  }
  update(view,width,height){
    if(!this.enabled || this.destroyed)return;
    this.view={...view};this.width=width;this.height=height;
    cancelAnimationFrame(this.pending);this.pending=0;this.draw();
  }
  schedule(){if(!this.pending && !this.destroyed)this.pending=requestAnimationFrame(()=>{this.pending=0;this.draw();});}
  select(id){this.selected=id;this.schedule();}
  hover(id){if(this.hovered!==id){this.hovered=id;this.schedule();}}
  hit(point){return this.hits?.hit(point)?.entity;}
  chunk(x,y,scale){
    const pixels=CHUNK_SIZE*scale,canvas=surface(pixels+2,pixels+2),ctx=canvas.getContext('2d');
    ctx.setTransform(scale,0,0,scale,1-x*CHUNK_SIZE*scale,1-y*CHUNK_SIZE*scale);
    for(const command of this.index.query({x:x*CHUNK_SIZE-1/scale,y:y*CHUNK_SIZE-1/scale,w:CHUNK_SIZE+2/scale,h:CHUNK_SIZE+2/scale})){
      if(command.seam){
        if(scale>=8){ctx.strokeStyle='#10171e80';ctx.lineWidth=.025;const r=command.bounds;ctx.strokeRect(r.x,r.y,r.w,r.h);}continue;
      }
      const image=this.frames.get(command.key);if(!image)continue;
      if(command.rotation){ctx.save();ctx.translate(command.ox,command.oy);ctx.rotate(command.rotation*Math.PI/180);}
      ctx.drawImage(image,command.x,command.y,command.w,command.h);
      if(command.rotation)ctx.restore();
    }
    return canvas;
  }
  draw(){
    if(!this.ready || !this.view || this.destroyed)return;
    const start=performance.now(),view=this.view,dpr=Math.min(devicePixelRatio || 1,2);
    const width=Math.max(1,Math.round(this.width*dpr)),height=Math.max(1,Math.round(this.height*dpr));
    if(this.canvas.width!==width || this.canvas.height!==height){this.canvas.width=width;this.canvas.height=height;}
    const ctx=this.context;ctx.setTransform(1,0,0,1,0,0);ctx.clearRect(0,0,width,height);
    ctx.setTransform(width/view.w,0,0,height/view.h,-view.x*width/view.w,-view.y*height/view.h);
    const visible=this.index.visible(view),scale=viewportResolution(view,width),pixels=CHUNK_SIZE*scale;
    let missing=false,built=0;
    for(const [x,y] of visible){
      const key=`${scale}:${x},${y}`;let tile=this.cache.get(key);
      if(!tile){
        // Bound new work per frame: a cold scene fills in over several frames
        // without blocking dragging, wheel events or the surrounding interface.
        if(built && performance.now()-start>8){
          missing=true;
          let fallback=cachedChunk(this.cache,x,y,scale);
          if(!fallback){
            const coarse=this.chunk(x,y,1);this.cache.set(`1:${x},${y}`,coarse,(CHUNK_SIZE+2)**2*4);
            fallback={tile:coarse,scale:1};
          }
          const source=CHUNK_SIZE*fallback.scale;
          ctx.drawImage(fallback.tile,1,1,source,source,x*CHUNK_SIZE,y*CHUNK_SIZE,CHUNK_SIZE,CHUNK_SIZE);
          continue;
        }
        tile=this.chunk(x,y,scale);this.cache.set(key,tile,(pixels+2)**2*4);built++;
      }
      ctx.drawImage(tile,1,1,pixels,pixels,x*CHUNK_SIZE,y*CHUNK_SIZE,CHUNK_SIZE,CHUNK_SIZE);
    }
    for(const [id,color,lineWidth] of [[this.hovered,'#ffffff',.08],[this.selected,'#edbd65',.13]]){
      const r=this.bounds.get(id)?.footprint;if(!r)continue;
      ctx.strokeStyle=color;ctx.lineWidth=lineWidth;ctx.strokeRect(r.x+.04,r.y+.04,r.w-.08,r.h-.08);
    }
    const elapsed=performance.now()-start;
    Object.assign(this.node.dataset,{renderMs:elapsed.toFixed(2),visibleChunks:String(visible.length),cachedChunks:String(this.cache.entries.size),cacheBytes:String(this.cache.bytes),pixelsPerTile:String(scale),pendingChunks:String(Number(missing))});
    if(performance.getEntriesByName('blueprint-render').length>=120)performance.clearMeasures('blueprint-render');
    performance.measure('blueprint-render',{start,end:performance.now()});
    if(missing)this.schedule();
  }
}
