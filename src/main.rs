//! The estimation game for a room of people.
//!
//! The host opens a room, puts the join link on the wall, and asks a
//! question. Each phone shows two fields. The host's screen shows the
//! room's bounds and, once the answer has been entered, how many of the
//! ranges contained it.
//!
//! There are three kinds of link, and the key in each is the only
//! authority there is: `/h/<key>` runs the room, `/j/<key>` joins it, and
//! `/p/<key>` belongs to one player.

mod game;
mod view;

use std::{path::PathBuf, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
};
use rooms::{Rooms, http, now_ms, qr};
use serde::Deserialize;

use game::{Game, Stage};

const INDEX: &str = include_str!("../static/index.html");
const ICON: &str = include_str!("../static/icon.svg");
/// The page's own half, compiled from the TypeScript in `client/` by the
/// build script. Nothing here is written in JavaScript.
const APP_JS: &str = include_str!(concat!(env!("OUT_DIR"), "/app.js"));
const APP_CSS: &str = include_str!(concat!(env!("OUT_DIR"), "/app.css"));

/// How many rooms at once. Limited because anyone can open one.
const MAX_ROOMS: usize = 2000;
/// How often to look for rooms nobody has opened in a day.
const SWEEP: Duration = Duration::from_secs(5 * 60);

type Shared = Arc<Rooms<Game>>;

fn refused(message: &str) -> Response {
    http::error(StatusCode::CONFLICT, message)
}

/// Applies a host action to the room and returns the host's view.
///
/// Every host request answers with the whole view, so the screen that
/// pressed the button does not wait for its own event stream.
async fn act(
    rooms: &Rooms<Game>,
    key: &str,
    action: impl FnOnce(&mut Game, u64) -> Result<(), &'static str>,
) -> Response {
    let done = rooms
        .with(key, |game, now| {
            action(game, now).map(|()| http::envelope(now, &view::host(game)))
        })
        .await;
    match done {
        None => http::gone(),
        Some(Err(message)) => refused(message),
        Some(Ok(body)) => http::json(body),
    }
}

#[derive(Deserialize)]
struct Asked {
    question: String,
    #[serde(default)]
    unit: String,
    /// The host can enter the answer before asking or afterwards.
    #[serde(default)]
    answer: Option<f64>,
}

#[derive(Deserialize)]
struct Staged {
    stage: Stage,
}

#[derive(Deserialize)]
struct Answered {
    answer: Option<f64>,
}

#[derive(Deserialize)]
struct Confidence {
    confidence: u8,
}

#[derive(Deserialize)]
struct Switch {
    on: bool,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct Bounds {
    low: f64,
    high: f64,
}

async fn open_room(State(rooms): State<Shared>) -> Response {
    let now = now_ms();
    let Some(host) = rooms.create(|_| Game::new(now)).await else {
        return http::error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Too many rooms are open at once. Try again in a few minutes.",
        );
    };
    let join = rooms.read(&host, |game, _| game.join.clone()).await;
    Json(serde_json::json!({ "host": host, "join": join })).into_response()
}

async fn host_state(State(rooms): State<Shared>, Path(key): Path<String>) -> Response {
    match rooms
        .read(&key, |game, now| http::envelope(now, &view::host(game)))
        .await
    {
        Some(body) => http::json(body),
        None => http::gone(),
    }
}

async fn host_events(State(rooms): State<Shared>, Path(key): Path<String>) -> Response {
    rooms::one(rooms, key, |game| {
        serde_json::to_string(&view::host(game)).ok()
    })
}

async fn ask(
    State(rooms): State<Shared>,
    Path(key): Path<String>,
    Json(asked): Json<Asked>,
) -> Response {
    act(&rooms, &key, |game, now| {
        game.ask(&asked.question, &asked.unit, asked.answer, now)
    })
    .await
}

/// Moves the question on: to the discussion, to the second ranges, or
/// back a step for somebody who missed one.
async fn set_stage(
    State(rooms): State<Shared>,
    Path(key): Path<String>,
    Json(staged): Json<Staged>,
) -> Response {
    act(&rooms, &key, |game, _| game.move_to(staged.stage)).await
}

async fn set_answer(
    State(rooms): State<Shared>,
    Path(key): Path<String>,
    Json(answered): Json<Answered>,
) -> Response {
    act(&rooms, &key, |game, _| game.set_answer(answered.answer)).await
}

async fn reveal(State(rooms): State<Shared>, Path(key): Path<String>) -> Response {
    act(&rooms, &key, |game, _| game.reveal()).await
}

async fn set_confidence(
    State(rooms): State<Shared>,
    Path(key): Path<String>,
    Json(level): Json<Confidence>,
) -> Response {
    act(&rooms, &key, |game, _| {
        game.set_confidence(level.confidence)
    })
    .await
}

/// Whether a player is told the answer as soon as they send their second
/// range, rather than waiting for the reveal.
async fn set_tell_on_send(
    State(rooms): State<Shared>,
    Path(key): Path<String>,
    Json(switch): Json<Switch>,
) -> Response {
    act(&rooms, &key, |game, _| {
        game.set_tell_on_send(switch.on);
        Ok(())
    })
    .await
}

/// A player's link carries the room's join key rather than the room's own
/// key, so the room has to be found by it.
async fn room_with_join(rooms: &Rooms<Game>, join: &str) -> Option<String> {
    let join = join.to_string();
    rooms
        .find(move |key, game| (game.join == join).then(|| key.to_string()))
        .await
}

async fn join(
    State(rooms): State<Shared>,
    Path(join): Path<String>,
    Json(named): Json<Named>,
) -> Response {
    let Some(key) = room_with_join(&rooms, &join).await else {
        return http::gone();
    };
    match rooms
        .with(&key, |game, now| game.join(&named.name, now))
        .await
    {
        None => http::gone(),
        Some(Err(message)) => refused(message),
        Some(Ok(player)) => Json(serde_json::json!({ "player": player })).into_response(),
    }
}

/// How many people are already in, which is what the join page shows
/// before anyone gives a name.
async fn whose(State(rooms): State<Shared>, Path(join): Path<String>) -> Response {
    let Some(key) = room_with_join(&rooms, &join).await else {
        return http::gone();
    };
    match rooms.read(&key, |game, _| game.players.len()).await {
        Some(players) => Json(serde_json::json!({ "players": players })).into_response(),
        None => http::gone(),
    }
}

async fn player_state(State(rooms): State<Shared>, Path(key): Path<String>) -> Response {
    let found = rooms
        .find(|_, game| view::player(game, &key).map(|view| http::envelope(now_ms(), &view)))
        .await;
    match found {
        Some(body) => http::json(body),
        None => http::gone(),
    }
}

async fn player_events(State(rooms): State<Shared>, Path(key): Path<String>) -> Response {
    rooms::stream(rooms, move |held, now| {
        let game = held.values_mut().find(|game| game.player(&key).is_some())?;
        game.last_seen = now;
        serde_json::to_string(&view::player(game, &key)?).ok()
    })
}

async fn bounds(
    State(rooms): State<Shared>,
    Path(key): Path<String>,
    Json(bounds): Json<Bounds>,
) -> Response {
    let Some(room) = rooms
        .find(|room, game| game.player(&key).map(|_| room.to_string()))
        .await
    else {
        return http::gone();
    };
    let done = rooms
        .with(&room, |game, now| {
            game.bound(&key, bounds.low, bounds.high, now)
                .map(|()| http::envelope(now, &view::player(game, &key)))
        })
        .await;
    match done {
        None => http::gone(),
        Some(Err(message)) => refused(message),
        Some(Ok(body)) => http::json(body),
    }
}

/// A link as a QR code. The page sends the link it wants, rather than
/// this guessing at the host name it is served under.
async fn qr_code(body: Bytes) -> Response {
    match std::str::from_utf8(&body) {
        Ok(link) => qr::response(link),
        Err(_) => http::error(StatusCode::BAD_REQUEST, "That is not a link."),
    }
}

/// The server's clock, which the pages run their countdowns off.
async fn time() -> Response {
    http::json(format!("{{\"now\":{}}}", now_ms()))
}

async fn page() -> Html<&'static str> {
    Html(INDEX)
}

/// The compiled page. The name has no hash in it, so it is revalidated
/// rather than cached; the file is small and a room is opened once.
async fn script() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        APP_JS,
    )
        .into_response()
}

async fn stylesheet() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        APP_CSS,
    )
        .into_response()
}

async fn icon() -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "max-age=3600"),
        ],
        ICON,
    )
        .into_response()
}

fn router(rooms: Shared) -> Router {
    Router::new()
        // One page, which picks its screen from the path.
        .route("/", get(page))
        .route("/h/{key}", get(page))
        .route("/j/{key}", get(page))
        .route("/p/{key}", get(page))
        .route("/icon.svg", get(icon))
        .route("/app.js", get(script))
        .route("/app.css", get(stylesheet))
        .route("/api/time", get(time))
        .route("/api/qr", post(qr_code))
        .route("/api/rooms", post(open_room))
        .route("/api/h/{key}", get(host_state))
        .route("/api/h/{key}/events", get(host_events))
        .route("/api/h/{key}/ask", post(ask))
        .route("/api/h/{key}/stage", put(set_stage))
        .route("/api/h/{key}/answer", put(set_answer))
        .route("/api/h/{key}/reveal", post(reveal))
        .route("/api/h/{key}/confidence", put(set_confidence))
        .route("/api/h/{key}/tell-on-send", put(set_tell_on_send))
        .route("/api/j/{key}", get(whose).post(join))
        .route("/api/p/{key}", get(player_state))
        .route("/api/p/{key}/events", get(player_events))
        .route("/api/p/{key}/bounds", put(bounds))
        .with_state(rooms)
}

#[tokio::main]
async fn main() {
    let addr = std::env::var("ESTIMATE_ADDR").unwrap_or_else(|_| "127.0.0.1:8097".into());
    let state = std::env::var("ESTIMATE_STATE").unwrap_or_else(|_| "rooms.json".into());

    let rooms: Shared = Rooms::load(PathBuf::from(state), MAX_ROOMS);
    tokio::spawn(rooms.clone().sweep(SWEEP));

    // Under systemd the port is held by a socket unit and passed in, so
    // the service itself runs with no network of its own.
    let listener = rooms::listen(&addr).await;
    println!("{}", rooms::where_it_listens(&listener));
    axum::serve(listener, router(rooms))
        .await
        .expect("the server stopped");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    /// A router with its own state file, which the test deletes after.
    fn app(name: &str) -> (Router, PathBuf) {
        let path =
            std::env::temp_dir().join(format!("estimate-test-{name}-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        (router(Rooms::load(&path, 4)), path)
    }

    async fn call(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let request = Request::builder().method(method).uri(uri);
        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("the request is well formed");
        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("the router answers");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    async fn ok(app: &Router, method: &str, uri: &str, body: Option<Value>) -> Value {
        let (status, body) = call(app, method, uri, body).await;
        assert!(status.is_success(), "{method} {uri}: {status} {body}");
        body
    }

    /// A room with one player in it and one question open.
    async fn ready(app: &Router) -> (String, String) {
        let keys = ok(app, "POST", "/api/rooms", None).await;
        let host = keys["host"].as_str().expect("a host key").to_string();
        let join = keys["join"].as_str().expect("a join key").to_string();
        let player = ok(
            app,
            "POST",
            &format!("/api/j/{join}"),
            Some(json!({"name": "Ada"})),
        )
        .await["player"]
            .as_str()
            .expect("a player key")
            .to_string();
        ok(
            app,
            "POST",
            &format!("/api/h/{host}/ask"),
            Some(json!({"question": "How many lamps?", "unit": "lamps", "answer": 1234567})),
        )
        .await;
        (host, player)
    }

    #[tokio::test]
    async fn a_question_goes_round_the_room_and_comes_back_as_numbers() {
        let (app, path) = app("round");
        let (host, player) = ready(&app).await;

        let view = ok(
            &app,
            "PUT",
            &format!("/api/p/{player}/bounds"),
            Some(json!({"low": 2000, "high": 3000})),
        )
        .await["view"]
            .clone();
        assert_eq!(view["mine_before"]["low"], 2000.0);
        assert_eq!(view["round"]["question"], "How many lamps?");
        assert_eq!(view["submitted"], 1);

        let view = ok(&app, "GET", &format!("/api/h/{host}"), None).await["view"].clone();
        assert_eq!(view["standing"]["n"], 1);
        assert_eq!(view["standing"]["median_low"], 2000.0);
        assert_eq!(view["standing"]["hits"], 0);
        assert_eq!(view["answers"][0]["name"], "Ada");
        assert_eq!(view["players"][0]["first"], true);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_player_is_not_sent_the_answer_until_it_is_revealed() {
        let (app, path) = app("reveal");
        let (host, player) = ready(&app).await;
        ok(
            &app,
            "PUT",
            &format!("/api/p/{player}/bounds"),
            Some(json!({"low": 1000000, "high": 2000000})),
        )
        .await;

        let before = ok(&app, "GET", &format!("/api/p/{player}"), None).await;
        assert!(
            !before.to_string().contains("1234567"),
            "the answer went out early: {before}"
        );

        ok(&app, "POST", &format!("/api/h/{host}/reveal"), None).await;
        let after = ok(&app, "GET", &format!("/api/p/{player}"), None).await["view"].clone();
        assert_eq!(after["round"]["answer"], 1234567.0);
        assert_eq!(after["hit"], true);
        assert_eq!(after["standing"]["n"], 1);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn the_room_answers_twice_and_the_change_is_counted() {
        let (app, path) = app("twice");
        let (host, player) = ready(&app).await;
        ok(
            &app,
            "PUT",
            &format!("/api/p/{player}/bounds"),
            Some(json!({"low": 0, "high": 10})),
        )
        .await;

        ok(
            &app,
            "PUT",
            &format!("/api/h/{host}/stage"),
            Some(json!({"stage": "discussion"})),
        )
        .await;
        let (status, body) = call(
            &app,
            "PUT",
            &format!("/api/p/{player}/bounds"),
            Some(json!({"low": 1, "high": 2})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "The room is talking this one over.");

        ok(
            &app,
            "PUT",
            &format!("/api/h/{host}/stage"),
            Some(json!({"stage": "second"})),
        )
        .await;
        let view = ok(
            &app,
            "PUT",
            &format!("/api/p/{player}/bounds"),
            Some(json!({"low": 1000000, "high": 2000000})),
        )
        .await["view"]
            .clone();
        assert_eq!(view["mine_before"]["high"], 10.0, "the first one stands");
        assert_eq!(view["mine_after"]["high"], 2000000.0);

        let view = ok(&app, "POST", &format!("/api/h/{host}/reveal"), None).await["view"].clone();
        assert_eq!(view["shift"]["n"], 1);
        assert_eq!(view["shift"]["moved"], 1);
        assert_eq!(view["shift"]["hits_before"], 0);
        assert_eq!(view["shift"]["hits_after"], 1, "1234567 is in the second");
        assert_eq!(view["shift"]["towards"], 1);
        assert_eq!(view["scores"][0]["changed_mind"], 1);

        let mine = ok(&app, "GET", &format!("/api/p/{player}"), None).await["view"].clone();
        assert_eq!(mine["mine_changed"]["after"]["low"], 1000000.0);
        assert_eq!(mine["hit"], true);
        assert_eq!(mine["hit_before"], false);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn the_room_can_be_told_the_answer_on_sending() {
        let (app, path) = app("tell");
        let (host, player) = ready(&app).await;
        ok(
            &app,
            "PUT",
            &format!("/api/h/{host}/tell-on-send"),
            Some(json!({"on": true})),
        )
        .await;
        ok(
            &app,
            "PUT",
            &format!("/api/h/{host}/stage"),
            Some(json!({"stage": "second"})),
        )
        .await;

        let before = ok(&app, "GET", &format!("/api/p/{player}"), None).await;
        assert!(before["view"]["round"]["answer"].is_null(), "not yet");

        let view = ok(
            &app,
            "PUT",
            &format!("/api/p/{player}/bounds"),
            Some(json!({"low": 1000000, "high": 2000000})),
        )
        .await["view"]
            .clone();
        assert_eq!(view["round"]["answer"], 1234567.0, "told on sending");
        assert!(view["standing"].is_null(), "and nothing else");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_revealed_question_takes_no_more_bounds() {
        let (app, path) = app("closed");
        let (host, player) = ready(&app).await;
        ok(&app, "POST", &format!("/api/h/{host}/reveal"), None).await;
        let (status, body) = call(
            &app,
            "PUT",
            &format!("/api/p/{player}/bounds"),
            Some(json!({"low": 1, "high": 2})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "This question is closed.");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn one_kind_of_key_does_not_work_as_another() {
        let (app, path) = app("keys");
        let (host, player) = ready(&app).await;
        for uri in [
            format!("/api/h/{player}"),
            format!("/api/p/{host}"),
            "/api/h/notakey".to_string(),
            "/api/p/notakey".to_string(),
            "/api/j/notakey".to_string(),
        ] {
            let (status, _) = call(&app, "GET", &uri, None).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        }
        let (status, _) = call(&app, "POST", &format!("/api/h/{player}/reveal"), None).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "a player cannot run the room"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn the_page_is_served_at_every_link_a_phone_might_be_handed() {
        let (app, path) = app("pages");
        for uri in ["/", "/h/abc", "/j/abc", "/p/abc"] {
            let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_room_survives_the_server_restarting() {
        let (app, path) = app("restart");
        let (host, player) = ready(&app).await;
        ok(
            &app,
            "PUT",
            &format!("/api/p/{player}/bounds"),
            Some(json!({"low": 1, "high": 9})),
        )
        .await;

        let again = router(Rooms::load(&path, 4));
        let view = ok(&again, "GET", &format!("/api/h/{host}"), None).await["view"].clone();
        assert_eq!(view["standing"]["n"], 1, "the room came back off disk");
        assert_eq!(view["answers"][0]["name"], "Ada");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_link_becomes_a_qr_code() {
        let (app, path) = app("qr");
        let request = Request::builder()
            .method("POST")
            .uri("/api/qr")
            .body(Body::from("https://estimate.rationality-munich.com/j/abc"))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&bytes).contains("<svg"));
        let _ = std::fs::remove_file(&path);
    }
}
