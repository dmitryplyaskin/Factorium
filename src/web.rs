//! Loopback-only UI/API. Generated artifacts are immutable snapshots; verification
//! belongs to their ID, never to the current contents of the browser form.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, State, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::{
    ExtractOptions, FactoryRequest, GameData, GeneratedFactory, SimulationOptions,
    generate::supported_recipe, generate_blueprint, verify_in_game,
};

pub struct WebOptions {
    pub port: u16,
    pub game: Option<ExtractOptions>,
}

#[derive(Clone)]
struct AppState {
    catalog: Arc<GameData>,
    catalog_response: Arc<Value>,
    game: Arc<Option<ExtractOptions>>,
    icons: Arc<HashMap<String, Vec<u8>>>,
    records: Arc<Mutex<VecDeque<Arc<Artifact>>>>,
    generation_slots: Arc<Semaphore>,
    game_slot: Arc<Semaphore>,
    port: u16,
}

struct Artifact {
    id: String,
    created_at: u64,
    factory: GeneratedFactory,
    blueprint_string: String,
    job: Mutex<Verification>,
}

#[derive(Clone, Default, Serialize)]
struct Verification {
    status: JobStatus,
    started_at: Option<u64>,
    finished_at: Option<u64>,
    report: Option<Value>,
    error: Option<String>,
}

#[derive(Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum JobStatus {
    #[default]
    NotStarted,
    Running,
    Complete,
    Error,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct VerifyRequest {
    warmup_seconds: u32,
    measurement_seconds: u32,
}
impl Default for VerifyRequest {
    fn default() -> Self {
        Self {
            warmup_seconds: 120,
            measurement_seconds: 120,
        }
    }
}

struct ApiError {
    status: StatusCode,
    message: String,
    detail: Option<String>,
}
impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            detail: None,
        }
    }
    fn detail(mut self, error: impl std::fmt::Display) -> Self {
        self.detail = Some(error.to_string());
        self
    }
    fn internal(error: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Не удалось выполнить операцию",
        )
        .detail(error)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({"error":{"message":self.message,"detail":self.detail}})),
        )
            .into_response()
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn option(e: &crate::model::Entity) -> Value {
    let normal = e.quality_stats.get("normal");
    let number = |key: &str| {
        normal
            .and_then(|v| v.get(key))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    json!({"name":e.prototype.name,"kind":e.prototype.prototype_type,"width":e.tile_width,"height":e.tile_height,
        "speed":number("crafting_speed"),"power_watts":number("max_energy_usage")*60.0,
        "supply_radius":number("supply_area_distance"),"wire_reach":number("max_wire_distance"),
        "belt_rate":e.prototype.properties.get("belt_speed").and_then(Value::as_f64).unwrap_or(0.0)*480.0})
}

fn build_catalog(data: &GameData, game_available: bool) -> Value {
    let machines: Vec<_> = data
        .entities
        .values()
        .filter(|e| {
            !e.prototype.hidden
                && matches!(
                    e.prototype.prototype_type.as_str(),
                    "assembling-machine" | "furnace"
                )
                && e.prototype
                    .properties
                    .contains_key("electric_energy_source")
                && e.tile_width <= 16
                && e.tile_height <= 16
        })
        .collect();
    let recipes:Vec<_>=data.recipes.values().filter_map(|r| {
        let compatible:Vec<_>=machines.iter().filter(|m|supported_recipe(data,r,m).is_ok()).map(|m|m.prototype.name.clone()).collect();
        if compatible.is_empty() {return None;}
        Some(json!({"name":r.prototype.name,"product":r.products[0].name,"input":r.ingredients[0].name,"input_amount":r.ingredients[0].amount,"output_amount":r.products[0].amount.or(r.products[0].amount_min),"seconds":r.energy,"machines":compatible}))
    }).collect();
    let equipment = |kind: &str| {
        data.entities
            .values()
            .filter(|e| !e.prototype.hidden && e.prototype.prototype_type == kind)
            .collect::<Vec<_>>()
    };
    let belts: Vec<_> = equipment("transport-belt")
        .into_iter()
        .filter(|e| e.tile_width == 1 && e.tile_height == 1)
        .map(option)
        .collect();
    let inserters: Vec<_> = equipment("inserter")
        .into_iter()
        .filter(|e| {
            e.tile_width == 1
                && e.tile_height == 1
                && e.prototype
                    .properties
                    .contains_key("electric_energy_source")
                && e.prototype.properties.get("inserter_pickup_position") == Some(&json!([0, -1]))
        })
        .map(option)
        .collect();
    let poles: Vec<_> = equipment("electric-pole")
        .into_iter()
        .filter(|e| e.tile_width <= 4 && e.tile_height <= 4)
        .map(option)
        .collect();
    json!({"game_version":data.game_version,"mods":data.mods,"recipe_count":data.recipes.len(),"recipes":recipes,
        "machines":machines.into_iter().map(option).collect::<Vec<_>>(),"belts":belts,"inserters":inserters,"poles":poles,
        "verification_available":game_available,"verification_hint":if game_available {"Factorio готова к проверке"} else {"Factorio не найдена. Запустите сервер с параметром --factorio."}})
}

fn load_icons(data: &GameData, game: Option<&ExtractOptions>) -> HashMap<String, Vec<u8>> {
    let Some(game) = game else {
        return HashMap::new();
    };
    let root = game.data_directory.clone().or_else(|| {
        game.factorio
            .parent()
            .and_then(std::path::Path::parent)
            .and_then(std::path::Path::parent)
            .map(|p| p.join("data"))
    });
    let Some(root) = root else {
        return HashMap::new();
    };
    let mut result = HashMap::new();
    for name in data.items.keys().chain(data.entities.keys()) {
        if !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            continue;
        }
        for module in ["base", "space-age", "quality", "elevated-rails", "recycler"] {
            let path = root
                .join(module)
                .join("graphics/icons")
                .join(format!("{name}.png"));
            if let Ok(bytes) = std::fs::read(path) {
                result.insert(name.clone(), bytes);
                break;
            }
        }
    }
    result
}

impl AppState {
    fn new(catalog: GameData, game: Option<ExtractOptions>, port: u16) -> Self {
        let available = game.as_ref().is_some_and(|g| g.factorio.is_file());
        let response = build_catalog(&catalog, available);
        let icons = load_icons(&catalog, game.as_ref());
        Self {
            catalog: Arc::new(catalog),
            catalog_response: Arc::new(response),
            game: Arc::new(game),
            icons: Arc::new(icons),
            records: Arc::new(Mutex::new(VecDeque::new())),
            generation_slots: Arc::new(Semaphore::new(2)),
            game_slot: Arc::new(Semaphore::new(1)),
            port,
        }
    }
    fn artifact(&self, id: &str) -> std::result::Result<Arc<Artifact>, ApiError> {
        self.records
            .lock()
            .map_err(ApiError::internal)?
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::NOT_FOUND,
                    "Чертёж не найден. Сгенерируйте его заново.",
                )
            })
    }
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/",get(||async{Html(include_str!("../web/index.html"))}))
        .route("/app.js",get(||async{asset("text/javascript; charset=utf-8",include_str!("../web/app.js"))}))
        .route("/model.js",get(||async{asset("text/javascript; charset=utf-8",include_str!("../web/model.js"))}))
        .route("/style.css",get(||async{asset("text/css; charset=utf-8",include_str!("../web/style.css"))}))
        .route("/favicon.svg",get(||async{asset("image/svg+xml",r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="8" fill="#efbe61"/><path d="M9 8h16v5H14v4h8v5h-8v5H9z" fill="#171b20"/></svg>"##)}))
        .route("/api/catalog",get(catalog))
        .route("/api/icons/{name}",get(icon))
        .route("/api/artifacts",get(history).post(generate))
        .route("/api/artifacts/{id}",get(artifact))
        .route("/api/artifacts/{id}/verification",get(verification).post(verify))
        .route("/api/artifacts/{id}/export/{format}",get(export))
        .fallback(||async{ApiError::new(StatusCode::NOT_FOUND,"Страница не найдена")})
        .layer(DefaultBodyLimit::max(16*1024))
        .layer(middleware::from_fn_with_state(state.clone(),local_requests))
        .with_state(state)
}

/// Build the local application without binding a socket (also used by API tests).
pub fn application(data: GameData, options: WebOptions) -> Router {
    router(AppState::new(data, options.game, options.port))
}

fn asset(content_type: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, content_type)], body).into_response()
}

async fn local_requests(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let allowed = [
        format!("127.0.0.1:{}", state.port),
        format!("localhost:{}", state.port),
    ];
    if !allowed.iter().any(|s| s == host) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "Этот сервер доступен только по локальному адресу",
        )
        .into_response();
    }
    if request.method() != axum::http::Method::GET && request.method() != axum::http::Method::HEAD {
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        if origin != Some(format!("http://{host}").as_str())
            || request
                .headers()
                .get("x-factorium-request")
                .and_then(|v| v.to_str().ok())
                != Some("1")
        {
            return ApiError::new(
                StatusCode::FORBIDDEN,
                "Откройте Factorium по локальному адресу и повторите действие",
            )
            .into_response();
        }
    }
    let mut response = next.run(request).await;
    for (name, value) in [
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("cache-control", "no-store"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

async fn catalog(State(s): State<AppState>) -> Json<Value> {
    Json((*s.catalog_response).clone())
}

async fn icon(State(s): State<AppState>, Path(name): Path<String>) -> Response {
    if let Some(bytes) = s.icons.get(&name) {
        // Icon PNGs can contain a horizontal mipmap chain. Crop its first square
        // with an SVG viewport instead of stretching all mip levels together.
        if bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
            if width > height && height > 0 {
                let svg = format!(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {height} {height}\"><image href=\"data:image/png;base64,{}\" width=\"{width}\" height=\"{height}\"/></svg>",
                    STANDARD.encode(bytes)
                );
                return ([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response();
            }
        }
        return ([(header::CONTENT_TYPE, "image/png")], bytes.clone()).into_response();
    }
    asset(
        "image/svg+xml",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path fill="#aab5c4" d="m32 4 24 14v28L32 60 8 46V18z"/><path fill="#4b5969" d="m32 32 24-14v28L32 60z"/><path fill="#788696" d="m32 32-24-14v28l24 14z"/></svg>"##,
    )
}

async fn generate(
    State(s): State<AppState>,
    payload: std::result::Result<Json<FactoryRequest>, JsonRejection>,
) -> std::result::Result<Json<Value>, ApiError> {
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
    let data = s.catalog.clone();
    let factory = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        generate_blueprint(&data, &request)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(|e| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Не удалось построить этот чертёж",
        )
        .detail(e)
    })?;
    let record = Arc::new(Artifact {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: now(),
        blueprint_string: factory.document.encode().map_err(ApiError::internal)?,
        factory,
        job: Mutex::new(Verification::default()),
    });
    let value = record_json(&record)?;
    let mut records = s.records.lock().map_err(ApiError::internal)?;
    if records.len() >= 20 {
        if let Some(i) = records
            .iter()
            .rposition(|r| r.job.lock().is_ok_and(|j| j.status != JobStatus::Running))
        {
            records.remove(i);
        }
    }
    records.push_front(record);
    Ok(Json(value))
}

fn record_json(r: &Artifact) -> std::result::Result<Value, ApiError> {
    Ok(
        json!({"id":r.id,"created_at":r.created_at,"factory":r.factory,"verification":r.job.lock().map_err(ApiError::internal)?.clone()}),
    )
}

async fn artifact(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> std::result::Result<Json<Value>, ApiError> {
    Ok(Json(record_json(s.artifact(&id)?.as_ref())?))
}

async fn history(State(s): State<AppState>) -> std::result::Result<Json<Value>, ApiError> {
    let rows=s.records.lock().map_err(ApiError::internal)?.iter().map(|r|Ok(json!({"id":r.id,"created_at":r.created_at,"product":r.factory.request.product,"rate":r.factory.request.rate,"verification":r.job.lock().map_err(ApiError::internal)?.clone()}))).collect::<std::result::Result<Vec<_>,ApiError>>()?;
    Ok(Json(json!({"artifacts":rows})))
}

async fn verification(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> std::result::Result<Json<Verification>, ApiError> {
    let record = s.artifact(&id)?;
    let job = record.job.lock().map_err(ApiError::internal)?.clone();
    Ok(Json(job))
}

async fn verify(
    State(s): State<AppState>,
    Path(id): Path<String>,
    payload: std::result::Result<Json<VerifyRequest>, JsonRejection>,
) -> std::result::Result<(StatusCode, Json<Verification>), ApiError> {
    let Json(input) = payload.map_err(|e| {
        ApiError::new(StatusCode::BAD_REQUEST, "Проверьте длительность испытания").detail(e)
    })?;
    if input.warmup_seconds > 30_000 || !(4..=30_000).contains(&input.measurement_seconds) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Прогрев: 0–30000 с; измерение: 4–30000 с",
        ));
    }
    if !s.catalog_response["verification_available"]
        .as_bool()
        .unwrap_or(false)
    {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Factorio не найдена. Укажите --factorio при запуске сервера.",
        ));
    }
    let record = s.artifact(&id)?;
    let permit = s.game_slot.clone().try_acquire_owned().map_err(|_| {
        ApiError::new(
            StatusCode::CONFLICT,
            "Уже идёт проверка в Factorio. Дождитесь её завершения.",
        )
    })?;
    let job = Verification {
        status: JobStatus::Running,
        started_at: Some(now()),
        ..Verification::default()
    };
    *record.job.lock().map_err(ApiError::internal)? = job.clone();
    tokio::spawn(async move {
        let _permit = permit;
        let data = s.catalog.clone();
        let game = s.game.clone();
        let artifact = record.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            verify_in_game(
                &data,
                &artifact.factory,
                game.as_ref().as_ref().expect("availability checked"),
                &SimulationOptions {
                    warmup_ticks: input.warmup_seconds * 60,
                    measurement_ticks: input.measurement_seconds * 60,
                },
            )
        })
        .await;
        if let Ok(mut job) = record.job.lock() {
            job.finished_at = Some(now());
            match outcome {
                Ok(Ok(report)) => {
                    job.status = JobStatus::Complete;
                    job.report = serde_json::to_value(report).ok();
                }
                Ok(Err(error)) => {
                    job.status = JobStatus::Error;
                    job.error = Some(format!("{error:#}"));
                }
                Err(error) => {
                    job.status = JobStatus::Error;
                    job.error = Some(error.to_string());
                }
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn export(
    State(s): State<AppState>,
    Path((id, format)): Path<(String, String)>,
) -> std::result::Result<Response, ApiError> {
    let r = s.artifact(&id)?;
    let (body, mime, extension, suffix) = match format.as_str() {
        "txt" => (
            r.blueprint_string.clone().into_bytes(),
            "text/plain; charset=utf-8",
            "txt",
            "",
        ),
        "json" => (
            serde_json::to_vec_pretty(&r.factory.document).map_err(ApiError::internal)?,
            "application/json",
            "json",
            "",
        ),
        "factory" => (
            serde_json::to_vec_pretty(&r.factory).map_err(ApiError::internal)?,
            "application/json",
            "json",
            "-factory",
        ),
        "svg" => (
            r.factory.preview_svg().into_bytes(),
            "image/svg+xml",
            "svg",
            "",
        ),
        "report" => {
            let job = r.job.lock().map_err(ApiError::internal)?;
            let report = job.report.as_ref().ok_or_else(|| {
                ApiError::new(StatusCode::CONFLICT, "Сначала завершите проверку чертежа")
            })?;
            (
                serde_json::to_vec_pretty(report).map_err(ApiError::internal)?,
                "application/json",
                "json",
                "-verification",
            )
        }
        _ => {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                "Неизвестный формат экспорта",
            ));
        }
    };
    let product: String = r
        .factory
        .request
        .product
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(80)
        .collect();
    let filename = format!("factorium-{product}-{id}{suffix}.{extension}");
    let mut response = Response::new(Body::from(body));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(ApiError::internal)?,
    );
    Ok(response)
}

pub async fn serve(data: GameData, options: WebOptions) -> Result<()> {
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, options.port)).await?;
    let port = listener.local_addr()?.port();
    println!("Factorium: http://127.0.0.1:{port}\nPress Ctrl+C to stop the local server.");
    axum::serve(listener, router(AppState::new(data, options.game, port)))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
