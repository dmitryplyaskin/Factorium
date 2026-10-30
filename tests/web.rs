mod common;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use factorium::{
    ExtractOptions, FactoryRequest,
    web::{WebOptions, application},
};
use serde_json::{Value, json};
use tower::ServiceExt;

fn app() -> Router {
    application(
        common::catalog(),
        WebOptions {
            port: 3410,
            game: None,
        },
    )
}
fn request(method: &str, path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:3410")
        .header("origin", "http://127.0.0.1:3410")
        .header("x-factorium-request", "1")
        .header("content-type", "application/json")
        .body(if method == "GET" {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap()
}
async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 10_000_000).await.unwrap()).unwrap()
}

#[tokio::test]
async fn transport_sprites_come_from_the_configured_game_and_missing_art_is_explicit() {
    let without_game = json_body(
        app()
            .oneshot(request("GET", "/api/catalog", Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(without_game["transport_sprites"], json!([]));

    let directory = tempfile::tempdir().unwrap();
    let sprite_directory = directory.path().join("base/graphics/entity/fast-splitter");
    std::fs::create_dir_all(&sprite_directory).unwrap();
    let png = b"\x89PNG\r\n\x1a\nfixture";
    std::fs::write(sprite_directory.join("fast-splitter-east.png"), png).unwrap();
    let mut game = ExtractOptions::new("unused-factorio");
    game.data_directory = Some(directory.path().to_owned());
    let router = application(
        common::catalog(),
        WebOptions {
            port: 3410,
            game: Some(game),
        },
    );
    let catalog = json_body(
        router
            .clone()
            .oneshot(request("GET", "/api/catalog", Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(catalog["transport_sprites"], json!(["fast-splitter-east"]));
    let sprite = router
        .clone()
        .oneshot(request(
            "GET",
            "/api/transport-sprites/fast-splitter-east",
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(sprite.status(), StatusCode::OK);
    assert_eq!(sprite.headers()["content-type"], "image/png");
    assert_eq!(&to_bytes(sprite.into_body(), 1000).await.unwrap()[..], png);
    let absent = router
        .oneshot(request(
            "GET",
            "/api/transport-sprites/fast-splitter-west",
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(absent.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn catalogs_only_compatible_recipes_and_generates_exportable_snapshots() {
    let app = app();
    let response = app
        .clone()
        .oneshot(request("GET", "/api/catalog", json!(null)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let catalog = json_body(response).await;
    assert_eq!(catalog["verification_available"], false);
    assert!(
        catalog["recipes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["name"] == "iron-gear-wheel")
    );
    assert_eq!(
        catalog["recipes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == "iron-plate")
            .unwrap()["machines"],
        json!(["electric-furnace"])
    );
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/artifacts",
            serde_json::to_value(FactoryRequest::new("iron-gear-wheel", 5.0)).unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let artifact = json_body(response).await;
    let id = artifact["id"].as_str().unwrap();
    assert_eq!(artifact["factory"]["input_per_second"], 10.0);
    assert_eq!(artifact["verification"]["status"], "not_started");
    for format in ["txt", "json", "factory", "svg"] {
        let response = app
            .clone()
            .oneshot(request(
                "GET",
                &format!("/api/artifacts/{id}/export/{format}"),
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["content-disposition"]
                .to_str()
                .unwrap()
                .contains("attachment;")
        );
        let bytes = to_bytes(response.into_body(), 10_000_000).await.unwrap();
        assert!(!bytes.is_empty());
        if format == "txt" {
            assert_eq!(bytes[0], b'0');
        }
        if format == "json" {
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes).unwrap(),
                artifact["factory"]["document"]
            );
        }
    }
    let missing_report = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/artifacts/{id}/export/report"),
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(missing_report.status(), StatusCode::CONFLICT);
    let verify = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/artifacts/{id}/verification"),
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::SERVICE_UNAVAILABLE);
    let history = json_body(
        app.oneshot(request("GET", "/api/artifacts", Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(history["artifacts"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn catalog_retains_game_recipe_groups_and_item_fallback() {
    let mut data = common::catalog();
    let recipe = &mut data
        .recipes
        .get_mut("iron-gear-wheel")
        .unwrap()
        .prototype
        .properties;
    recipe.insert("group".into(), json!("intermediate-products"));
    recipe.insert("subgroup".into(), json!("intermediate-product"));
    recipe.insert("order".into(), json!("a[basic]-a[gear]"));
    let item = &mut data
        .items
        .get_mut("iron-plate")
        .unwrap()
        .prototype
        .properties;
    item.insert("group".into(), json!("modded-materials"));
    item.insert("subgroup".into(), json!("plates"));
    let app = application(
        data,
        WebOptions {
            port: 3410,
            game: None,
        },
    );
    let catalog = json_body(
        app.oneshot(request("GET", "/api/catalog", Value::Null))
            .await
            .unwrap(),
    )
    .await;
    let recipes = catalog["recipes"].as_array().unwrap();
    let gear = recipes
        .iter()
        .find(|r| r["name"] == "iron-gear-wheel")
        .unwrap();
    assert_eq!(gear["group"], "intermediate-products");
    assert_eq!(gear["subgroup"], "intermediate-product");
    assert_eq!(gear["order"], "a[basic]-a[gear]");
    let plate = recipes.iter().find(|r| r["name"] == "iron-plate").unwrap();
    assert_eq!(plate["group"], "modded-materials");
    assert_eq!(plate["subgroup"], "plates");
}

#[tokio::test]
async fn catalog_and_exports_include_both_solid_ingredients() {
    let app = app();
    let catalog = json_body(
        app.clone()
            .oneshot(request("GET", "/api/catalog", Value::Null))
            .await
            .unwrap(),
    )
    .await;
    let recipe = catalog["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "electronic-circuit")
        .unwrap();
    assert_eq!(recipe["ingredients"].as_array().unwrap().len(), 2);
    assert!(!catalog["long_inserters"].as_array().unwrap().is_empty());
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/artifacts",
            serde_json::to_value(FactoryRequest::new("electronic-circuit", 1.0)).unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let artifact = json_body(response).await;
    assert_eq!(artifact["factory"]["inputs"].as_array().unwrap().len(), 2);
    assert_eq!(
        artifact["factory"]["rows"][0]["additional_inputs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let path = format!(
        "/api/artifacts/{}/export/factory",
        artifact["id"].as_str().unwrap()
    );
    let exported = json_body(
        app.oneshot(request("GET", &path, Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(exported, artifact["factory"]);
}

#[tokio::test]
async fn validation_errors_are_json_and_do_not_create_successful_artifacts() {
    let app = app();
    let mut bad = FactoryRequest::new("iron-gear-wheel", 5.0);
    bad.rate = -1.0;
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/artifacts",
            serde_json::to_value(bad).unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(json_body(response).await["error"]["detail"].is_string());
    let response = app
        .clone()
        .oneshot(request("POST", "/api/artifacts", json!({})))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = app
        .clone()
        .oneshot(request("GET", "/api/artifacts/999", Value::Null))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let history = json_body(
        app.oneshot(request("GET", "/api/artifacts", Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert!(history["artifacts"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn artifact_ids_cannot_refer_to_another_server_session() {
    let mut ids = Vec::new();
    for _ in 0..2 {
        let response = app()
            .oneshot(request(
                "POST",
                "/api/artifacts",
                serde_json::to_value(FactoryRequest::new("iron-gear-wheel", 5.0)).unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let artifact = json_body(response).await;
        ids.push(artifact["id"].as_str().unwrap().to_owned());
    }
    assert_ne!(ids[0], ids[1]);
}

#[tokio::test]
async fn forbids_foreign_origins_and_dns_rebinding_hosts() {
    for (header, value) in [
        ("host", "foreign.example:3410"),
        ("origin", "https://foreign.example"),
        ("x-factorium-request", "0"),
    ] {
        let mut req = request(
            "POST",
            "/api/artifacts",
            serde_json::to_value(FactoryRequest::new("iron-gear-wheel", 5.0)).unwrap(),
        );
        req.headers_mut().insert(header, value.parse().unwrap());
        let response = app().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let response = app()
        .oneshot(request("GET", "/", Value::Null))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("content-security-policy"));
}

#[tokio::test]
async fn retains_independent_snapshots_and_bounds_the_session_history() {
    let app = app();
    let mut ids = Vec::new();
    for rate in 1..=22 {
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/artifacts",
                serde_json::to_value(FactoryRequest::new("iron-gear-wheel", f64::from(rate)))
                    .unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        ids.push(json_body(response).await["id"].as_str().unwrap().to_owned());
    }
    let response = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/artifacts/{}", ids[0]),
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let snapshot = json_body(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/artifacts/{}", ids[20]),
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(snapshot["factory"]["request"]["rate"], 21.0);
    assert_eq!(snapshot["verification"]["status"], "not_started");
    let history = json_body(
        app.oneshot(request("GET", "/api/artifacts", Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(history["artifacts"].as_array().unwrap().len(), 20);
}

#[tokio::test]
async fn three_ingredient_recipes_expose_materials_and_shared_lanes_through_the_api() {
    let app = app();
    let catalog = json_body(
        app.clone()
            .oneshot(request("GET", "/api/catalog", Value::Null))
            .await
            .unwrap(),
    )
    .await;
    let recipe = catalog["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "advanced-circuit")
        .unwrap();
    assert_eq!(recipe["ingredients"].as_array().unwrap().len(), 3);
    let mut factory_request = FactoryRequest::new("advanced-circuit", 0.5);
    factory_request.common_input = true;
    let response = app
        .oneshot(request(
            "POST",
            "/api/artifacts",
            serde_json::to_value(factory_request).unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let artifact = json_body(response).await;
    let factory: factorium::GeneratedFactory =
        serde_json::from_value(artifact["factory"].clone()).unwrap();
    assert_eq!(factory.input_belt_count(), 2);
    assert_eq!(factory.external_inputs().len(), 3);
    assert_eq!(
        factory
            .external_inputs()
            .iter()
            .filter(|p| p.lanes != "both")
            .count(),
        2
    );
}
