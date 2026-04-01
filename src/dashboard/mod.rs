use crate::error::ForgeError;
use crate::model::{Blueprint, Step};
use axum::extract::State;
use axum::response::Html;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::{Json, Router};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::json;
use std::convert::Infallible;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

pub type SharedDashboardState = Arc<Mutex<DashboardState>>;

const MAX_PORT: u16 = 8420;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct DashboardState {
    pub blueprint_name: String,
    pub started_at: Instant,
    pub steps: Vec<StepState>,
    pub current_step: Option<usize>,
    pub finished: bool,
    pub final_status: Option<String>,
    pub finished_at: Option<Instant>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepState {
    pub name: String,
    pub step_type: String,
    pub status: StepStatus,
    pub output: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct DashboardObserver {
    state: SharedDashboardState,
    events: broadcast::Sender<DashboardEvent>,
}

pub struct DashboardServer {
    pub observer: DashboardObserver,
    pub port: u16,
    join_handle: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub enum DashboardEvent {
    StepStart {
        step: String,
        index: usize,
    },
    StepEnd {
        step: String,
        status: StepStatus,
        duration_ms: Option<u64>,
    },
    RunComplete {
        status: String,
    },
}

#[derive(Clone)]
struct AppState {
    dashboard: SharedDashboardState,
    events: broadcast::Sender<DashboardEvent>,
}

impl Serialize for DashboardState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("DashboardState", 7)?;
        state.serialize_field("blueprint_name", &self.blueprint_name)?;
        state.serialize_field("started_at_ms", &self.started_at.elapsed().as_millis())?;
        state.serialize_field("steps", &self.steps)?;
        state.serialize_field("current_step", &self.current_step)?;
        state.serialize_field("finished", &self.finished)?;
        state.serialize_field("final_status", &self.final_status)?;
        state.serialize_field(
            "finished_at_ms",
            &self
                .finished_at
                .map(|finished_at| finished_at.elapsed().as_millis()),
        )?;
        state.end()
    }
}

impl DashboardState {
    pub fn from_blueprint(blueprint: &Blueprint) -> Self {
        Self::new(&blueprint.blueprint.name, blueprint.steps.as_slice())
    }

    pub fn new(blueprint_name: &str, steps: &[Step]) -> Self {
        Self {
            blueprint_name: blueprint_name.to_string(),
            started_at: Instant::now(),
            steps: steps
                .iter()
                .map(|step| StepState {
                    name: step.name.clone(),
                    step_type: format!("{:?}", step.step_type).to_ascii_lowercase(),
                    status: StepStatus::Pending,
                    output: None,
                    duration_ms: None,
                })
                .collect(),
            current_step: None,
            finished: false,
            final_status: None,
            finished_at: None,
        }
    }
}

impl DashboardObserver {
    pub fn start_step(&self, index: usize, name: &str) {
        let mut state = self.state.lock().expect("dashboard state lock poisoned");
        if let Some(step) = state.steps.get_mut(index) {
            step.status = StepStatus::Running;
            step.output = None;
            step.duration_ms = None;
            state.current_step = Some(index);
        }
        drop(state);
        let _ = self.events.send(DashboardEvent::StepStart {
            step: name.to_string(),
            index,
        });
    }

    pub fn finish_step(
        &self,
        index: usize,
        name: &str,
        status: StepStatus,
        output: Option<String>,
        duration_ms: Option<u64>,
    ) {
        let mut state = self.state.lock().expect("dashboard state lock poisoned");
        if let Some(step) = state.steps.get_mut(index) {
            step.status = status.clone();
            step.output = output;
            step.duration_ms = duration_ms;
        }
        state.current_step = None;
        drop(state);
        let _ = self.events.send(DashboardEvent::StepEnd {
            step: name.to_string(),
            status,
            duration_ms,
        });
    }

    pub fn complete_run(&self, status: &str) {
        let mut state = self.state.lock().expect("dashboard state lock poisoned");
        state.finished = true;
        state.final_status = Some(status.to_string());
        state.finished_at = Some(Instant::now());
        state.current_step = None;
        drop(state);
        let _ = self.events.send(DashboardEvent::RunComplete {
            status: status.to_string(),
        });
    }

    pub fn shared_state(&self) -> SharedDashboardState {
        Arc::clone(&self.state)
    }
}

impl DashboardServer {
    pub fn wait(mut self) -> Result<(), ForgeError> {
        if let Some(handle) = self.join_handle.take() {
            handle
                .join()
                .map_err(|_| ForgeError::message("dashboard thread panicked"))?;
        }
        Ok(())
    }
}

pub fn create_dashboard_state(blueprint: &Blueprint) -> SharedDashboardState {
    Arc::new(Mutex::new(DashboardState::from_blueprint(blueprint)))
}

pub fn launch_dashboard(
    blueprint: &Blueprint,
    preferred_port: u16,
) -> Result<DashboardServer, ForgeError> {
    let state = create_dashboard_state(blueprint);
    let (events, _) = broadcast::channel(64);
    let (port, join_handle) =
        start_dashboard_with_events(Arc::clone(&state), events.clone(), preferred_port)?;

    Ok(DashboardServer {
        observer: DashboardObserver { state, events },
        port,
        join_handle: Some(join_handle),
    })
}

pub fn start_dashboard(
    state: SharedDashboardState,
    preferred_port: u16,
) -> Result<(u16, JoinHandle<()>), ForgeError> {
    let (events, _) = broadcast::channel(64);
    start_dashboard_with_events(state, events, preferred_port)
}

fn start_dashboard_with_events(
    state: SharedDashboardState,
    events: broadcast::Sender<DashboardEvent>,
    preferred_port: u16,
) -> Result<(u16, JoinHandle<()>), ForgeError> {
    let (port, listener) = bind_dashboard_listener(preferred_port)?;
    eprintln!("Dashboard: http://localhost:{port}");

    let join_handle = thread::spawn(move || {
        let runtime = match Runtime::new() {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("failed to start dashboard runtime: {error}");
                return;
            }
        };

        runtime.block_on(async move {
            let listener = match tokio::net::TcpListener::from_std(listener) {
                Ok(listener) => listener,
                Err(error) => {
                    eprintln!("failed to convert dashboard listener: {error}");
                    return;
                }
            };

            let app = Router::new()
                .route("/", get(index))
                .route("/api/state", get(state_snapshot))
                .route("/events", get(events_stream))
                .with_state(AppState {
                    dashboard: Arc::clone(&state),
                    events: events.clone(),
                });

            let server =
                axum::serve(listener, app).with_graceful_shutdown(wait_for_shutdown(state));
            if let Err(error) = server.await {
                eprintln!("dashboard server error: {error}");
            }
        });
    });

    Ok((port, join_handle))
}

fn bind_dashboard_listener(preferred_port: u16) -> Result<(u16, TcpListener), ForgeError> {
    for port in preferred_port..=MAX_PORT {
        match TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))) {
            Ok(listener) => {
                listener.set_nonblocking(true)?;
                return Ok((port, listener));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Err(ForgeError::message(format!(
        "failed to bind dashboard port in range {preferred_port}-{MAX_PORT}"
    )))
}

async fn wait_for_shutdown(state: SharedDashboardState) {
    loop {
        let should_shutdown = {
            let state = state.lock().expect("dashboard state lock poisoned");
            state.finished
                && state
                    .finished_at
                    .is_some_and(|finished_at| finished_at.elapsed() >= SHUTDOWN_GRACE)
        };

        if should_shutdown {
            return;
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn index() -> Html<&'static str> {
    Html(
        "<!DOCTYPE html><html><head><title>Forge Dashboard</title></head><body><h1>Forge Dashboard</h1><p>Dashboard UI coming soon. API available at /api/state</p></body></html>",
    )
}

async fn state_snapshot(State(app): State<AppState>) -> Result<Json<DashboardState>, ForgeError> {
    let state = app.dashboard.lock().expect("dashboard state lock poisoned");
    Ok(Json(state.clone()))
}

async fn events_stream(
    State(app): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(app.events.subscribe()).filter_map(|message| match message {
        Ok(event) => Some(Ok(event.into_sse())),
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

impl DashboardEvent {
    fn into_sse(self) -> Event {
        match self {
            Self::StepStart { step, index } => Event::default()
                .event("step_start")
                .data(json!({ "step": step, "index": index }).to_string()),
            Self::StepEnd {
                step,
                status,
                duration_ms,
            } => Event::default().event("step_end").data(
                json!({
                    "step": step,
                    "status": status,
                    "duration_ms": duration_ms,
                })
                .to_string(),
            ),
            Self::RunComplete { status } => Event::default()
                .event("run_complete")
                .data(json!({ "status": status }).to_string()),
        }
    }
}

impl IntoResponse for ForgeError {
    fn into_response(self) -> axum::response::Response {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            self.to_string(),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BlueprintMeta, StepType};
    use std::collections::BTreeMap;

    #[test]
    fn dashboard_state_serializes_to_json() {
        let blueprint = Blueprint {
            blueprint: BlueprintMeta {
                name: "demo".to_string(),
                description: "demo".to_string(),
                repos: Vec::new(),
            },
            steps: vec![Step {
                step_type: StepType::Deterministic,
                name: "lint".to_string(),
                command: Some("cargo clippy".to_string()),
                agent: None,
                model: None,
                prompt: None,
                blueprint: None,
                params: BTreeMap::new(),
                condition: None,
                sets: None,
                allow_failure: false,
                max_retries: None,
                expect_failure: false,
                env: BTreeMap::new(),
            }],
            source_path: None,
        };
        let state = DashboardState::from_blueprint(&blueprint);
        let json = serde_json::to_value(state).expect("dashboard state should serialize");

        assert_eq!(json["blueprint_name"], "demo");
        assert_eq!(json["steps"][0]["name"], "lint");
        assert_eq!(json["steps"][0]["status"], "pending");
        assert!(json.get("started_at_ms").is_some());
    }

    #[test]
    fn port_selection_uses_next_available_port() {
        let occupied = TcpListener::bind(("127.0.0.1", 8410)).expect("bind occupied port");
        let (port, listener) = bind_dashboard_listener(8410).expect("should find open port");

        assert_eq!(port, 8411);
        drop(listener);
        drop(occupied);
    }
}
