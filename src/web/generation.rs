//! Generation jobs keep the UI responsive and cancel the actual CPU work.

use super::*;
use crate::generate::{GenerationControl, generate_blueprint_controlled};

pub(super) struct GenerationJob {
    id: String,
    control: GenerationControl,
    outcome: Mutex<Outcome>,
}

#[derive(Default)]
struct Outcome {
    finished: bool,
    artifact_id: Option<String>,
    error: Option<String>,
}

impl GenerationJob {
    fn snapshot(&self) -> std::result::Result<Value, ApiError> {
        let outcome = self.outcome.lock().map_err(ApiError::internal)?;
        Ok(self.value(&outcome))
    }

    fn value(&self, outcome: &Outcome) -> Value {
        let status = if outcome.artifact_id.is_some() {
            "complete"
        } else if self.control.is_cancelled() {
            "cancelled"
        } else if outcome.finished {
            "error"
        } else {
            "running"
        };
        json!({"id":self.id,"status":status,"progress":self.control.progress(),
            "artifact_id":outcome.artifact_id,"error":outcome.error})
    }
}

fn find(s: &AppState, id: &str) -> std::result::Result<Arc<GenerationJob>, ApiError> {
    s.generations
        .lock()
        .map_err(ApiError::internal)?
        .iter()
        .find(|job| job.id == id)
        .cloned()
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "Расчёт не найден. Запустите генерацию заново.",
            )
        })
}

pub(super) async fn start(
    State(s): State<AppState>,
    payload: std::result::Result<Json<FactoryRequest>, JsonRejection>,
) -> std::result::Result<(StatusCode, Json<Value>), ApiError> {
    let Json(request) = payload.map_err(|e| {
        ApiError::new(StatusCode::BAD_REQUEST, "Проверьте параметры генерации").detail(e)
    })?;
    let permit = s
        .generation_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "Генератор занят. Повторите через несколько секунд.",
            )
        })?;
    let job = Arc::new(GenerationJob {
        id: uuid::Uuid::new_v4().to_string(),
        control: GenerationControl::default(),
        outcome: Mutex::new(Outcome::default()),
    });
    {
        let mut jobs = s.generations.lock().map_err(ApiError::internal)?;
        while jobs.len() >= 20 {
            if let Some(i) = jobs
                .iter()
                .rposition(|j| j.outcome.lock().is_ok_and(|o| o.finished))
            {
                jobs.remove(i);
            } else {
                break;
            }
        }
        jobs.push_front(job.clone());
    }
    let response = job.snapshot()?;
    tokio::spawn(async move {
        let data = s.catalog.clone();
        let worker = job.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            generate_blueprint_controlled(&data, &request, &worker.control)
        })
        .await;
        // Serialize completion with cancellation: an accepted cancellation must
        // never publish a late blueprint or add it to history.
        let Ok(mut outcome) = job.outcome.lock() else {
            return;
        };
        outcome.finished = true;
        if job.control.is_cancelled() {
            return;
        }
        match result {
            Ok(Ok(factory)) => match record_factory(&s, factory) {
                Ok(artifact) => outcome.artifact_id = Some(artifact.id.clone()),
                Err(error) => outcome.error = Some(error.message),
            },
            Ok(Err(error)) => {
                outcome.error = Some(format!("Не удалось построить этот чертёж: {error:#}"))
            }
            Err(error) => outcome.error = Some(format!("Ошибка генератора: {error}")),
        }
    });
    Ok((StatusCode::ACCEPTED, Json(response)))
}

pub(super) async fn status(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> std::result::Result<Json<Value>, ApiError> {
    Ok(Json(find(&s, &id)?.snapshot()?))
}

pub(super) async fn cancel(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> std::result::Result<Json<Value>, ApiError> {
    let job = find(&s, &id)?;
    let outcome = job.outcome.lock().map_err(ApiError::internal)?;
    if !outcome.finished {
        job.control.cancel();
    }
    Ok(Json(job.value(&outcome)))
}
