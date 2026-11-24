// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! `integration/work-items.toml` against the real router: report-rs reads that file, so a
//! route it names that this server does not serve, or a create body key the handler does not
//! read, must fail here and not in report-rs.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Method, Request};
use serde::Deserialize;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::health::{AppState, HealthChecker, HealthConfig};
use crate::metrics::Metrics;
use op_api::handlers::work_packages::CreateWorkPackageDto;
use op_core::config::AppConfig;

const MAPPING: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../integration/work-items.toml"
);

#[derive(Deserialize)]
struct Mapping {
    schema: u32,
    name: String,
    create: Call,
    list: Call,
    item: Call,
    comments: Call,
}

#[derive(Deserialize)]
struct Call {
    method: String,
    path: String,
    body: Option<toml::Table>,
}

fn mapping() -> Mapping {
    let text = std::fs::read_to_string(MAPPING).expect("integration/work-items.toml is readable");
    toml::from_str(&text).expect("integration/work-items.toml parses")
}

fn app() -> axum::Router {
    let state = Arc::new(AppState {
        health: Arc::new(HealthChecker::new(HealthConfig::default())),
        config: AppConfig::default(),
        db: None,
        desk: crate::desk_pages::shared(crate::desk_pages::load_desk(|_| None).expect("demo desk")),
    });
    crate::build_router_with(state, Arc::new(Metrics::new()), None)
}

/// The create body with `{project}` as the number 1 and `{title}`, `{description}` as strings.
fn create_body(call: &Call) -> Value {
    let body = serde_json::to_value(call.body.as_ref().expect("create has a body")).unwrap();
    let Value::Object(map) = body else {
        panic!("the create body is an object")
    };
    Value::Object(
        map.into_iter()
            .map(|(key, value)| {
                let filled = match value.as_str() {
                    Some("{project}") => json!(1),
                    Some("{title}") => json!("A title"),
                    Some("{description}") => json!("A description"),
                    _ => value,
                };
                (key, filled)
            })
            .collect(),
    )
}

async fn status_of(method: &str, path: &str, body: Option<&Value>) -> u16 {
    let path = path.replace("{id}", "1").replace("{project}", "1");
    let mut req = Request::builder()
        .method(Method::from_bytes(method.as_bytes()).expect("a method"))
        .uri(&path);
    let body = match body {
        Some(b) => {
            req = req.header(header::CONTENT_TYPE, "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    app()
        .oneshot(req.body(body).unwrap())
        .await
        .unwrap()
        .status()
        .as_u16()
}

/// FAILS IF: a method and path named in integration/work-items.toml is not served by the
/// router (404 or 405), or the create body names a key `CreateWorkPackageDto` does not read.
#[tokio::test]
async fn the_work_item_mapping_names_served_routes() {
    let m = mapping();
    assert_eq!((m.schema, m.name.as_str()), (1, "openproject"));

    // Control: the check can fail: a path the router does not serve is 404.
    assert_eq!(
        status_of("GET", "/api/v3/work_packages_nope", None).await,
        404
    );

    let body = create_body(&m.create);
    for (what, call, body) in [
        ("create", &m.create, Some(&body)),
        ("list", &m.list, None),
        ("item", &m.item, None),
        ("comments", &m.comments, None),
    ] {
        let status = status_of(&call.method, &call.path, body).await;
        assert!(
            status != 404 && status != 405,
            "{what}: {} {} answers {status}",
            call.method,
            call.path
        );
    }

    // The keys of the create body are the fields the handler reads, not look-alikes.
    let dto: CreateWorkPackageDto =
        serde_json::from_value(body.clone()).expect("the create body is a CreateWorkPackageDto");
    assert_eq!(dto.project_id, Some(1));
    assert_eq!(dto.subject, "A title");
    assert_eq!(dto.description.as_deref(), Some("A description"));
    assert_eq!(body.as_object().unwrap().len(), 3);
}
