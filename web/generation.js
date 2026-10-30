// A job owns its server calculation until completion or acknowledged cancellation.
export class GenerationTask {
  constructor(api,{onProgress=()=>{},onCancelError=()=>{},wait=ms=>new Promise(resolve=>setTimeout(resolve,ms))}={}) {
    this.api=api;this.onProgress=onProgress;this.onCancelError=onCancelError;this.wait=wait;
    this.id=null;this.cancelRequested=false;this.cancelPromise=null;
  }
  async cancel() {
    this.cancelRequested=true;
    if(!this.id)return;
    if(this.cancelPromise)return this.cancelPromise;
    this.cancelPromise=this.api(`/api/generations/${this.id}/cancel`,{method:'POST',body:'{}'});
    try {return await this.cancelPromise;}
    catch(error){this.cancelRequested=false;throw error;}
    finally {this.cancelPromise=null;}
  }
  async run(input) {
    let job=await this.api('/api/generations',{method:'POST',body:JSON.stringify(input)});
    this.id=job.id;
    if(this.cancelRequested)await this.cancel().catch(error=>this.onCancelError(error));
    try {
      for(;;) {
        this.onProgress(job);
        if(job.status==='cancelled')return null;
        if(job.status==='error')throw new Error(job.error || 'Не удалось построить чертёж');
        if(job.status==='complete')return this.api(`/api/artifacts/${job.artifact_id}`);
        await this.wait(300);
        job=await this.api(`/api/generations/${this.id}`);
      }
    } finally {
      if(job.status==='running')await this.cancel().catch(()=>{});
    }
  }
}

export function generationProgress(progress={},seconds=0) {
  const stage={layout:'Строим линию',bus:'Уплотняем шину',power:'Рассчитываем электросеть',validation:'Проверяем чертёж'}[progress.stage] || 'Подготавливаем расчёт';
  return [stage,progress.machines?`${progress.machines} машин`:null,
    progress.candidates?`вариант ${progress.candidate} из ${progress.candidates}`:null,`${seconds} с`].filter(Boolean).join(' · ');
}
