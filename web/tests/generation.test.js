import test from 'node:test';
import assert from 'node:assert/strict';
import {GenerationTask,generationProgress} from '../generation.js';

const running={id:'job',status:'running',progress:{machines:739,candidate:2,candidates:50,stage:'power'}};

test('generation reports progress and loads the immutable completed artifact',async()=>{
  const calls=[],progress=[],artifact={id:'blueprint'};
  const task=new GenerationTask(async(path,options)=>{
    calls.push([path,options?.method]);
    if(path==='/api/generations')return running;
    if(path==='/api/generations/job')return {...running,status:'complete',artifact_id:'blueprint'};
    if(path==='/api/artifacts/blueprint')return artifact;
    assert.fail(path);
  },{wait:async()=>{},onProgress:job=>progress.push(job.status)});
  assert.equal(await task.run({rate:15}),artifact);
  assert.deepEqual(progress,['running','complete']);
  assert.equal(calls.length,3);
  assert.match(generationProgress(running.progress,4),/739 машин.*2 из 50.*4 с/);
});

test('cancel before the start response reaches the server job and never loads an artifact',async()=>{
  let started,release;const calls=[];
  const pending=new Promise(resolve=>{release=resolve;});
  const task=new GenerationTask(async path=>{
    calls.push(path);
    if(path==='/api/generations'){started=true;await pending;return running;}
    if(path.endsWith('/cancel'))return {...running,status:'cancelled'};
    if(path==='/api/generations/job')return {...running,status:'cancelled'};
    assert.fail(path);
  },{wait:async()=>{}});
  const result=task.run({});assert.equal(started,true);
  await task.cancel();release();
  assert.equal(await result,null);
  assert.deepEqual(calls,['/api/generations','/api/generations/job/cancel','/api/generations/job']);
});

test('a failed cancellation can be retried while polling continues',async()=>{
  let cancelled=false,attempts=0;const errors=[];
  const task=new GenerationTask(async path=>{
    if(path==='/api/generations')return running;
    if(path.endsWith('/cancel')){
      if(++attempts===1)throw new Error('temporary failure');
      cancelled=true;return {...running,status:'cancelled'};
    }
    return {...running,status:cancelled?'cancelled':'running'};
  },{wait:async()=>{if(!cancelled)await task.cancel();},onCancelError:error=>errors.push(error.message)});
  await task.cancel();
  assert.equal(await task.run({}),null);
  assert.deepEqual(errors,['temporary failure']);assert.equal(attempts,2);
});

test('losing polling cancels the calculation before surfacing the connection error',async()=>{
  const calls=[];
  const task=new GenerationTask(async path=>{
    calls.push(path);
    if(path==='/api/generations')return running;
    if(path.endsWith('/cancel'))return {...running,status:'cancelled'};
    throw new Error('offline');
  },{wait:async()=>{}});
  await assert.rejects(task.run({}),/offline/);
  assert.equal(calls.at(-1),'/api/generations/job/cancel');
});

test('server validation errors remain visible to the user',async()=>{
  const task=new GenerationTask(async()=>({...running,status:'error',error:'Unsupported recipe'}));
  await assert.rejects(task.run({}),/Unsupported recipe/);
});
