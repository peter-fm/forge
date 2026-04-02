use forge::dashboard::launch_dashboard;
use forge::model::{Blueprint, BlueprintMeta, Step, StepType};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn dashboard_api_state_returns_valid_json() {
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

    let server = launch_dashboard(&blueprint, 8412).expect("dashboard should start");
    let body = get_body(server.port, "/api/state");
    let json: Value = serde_json::from_str(&body).expect("response should be json");

    assert_eq!(json["blueprint_name"], "demo");
    assert_eq!(json["steps"][0]["name"], "lint");
    assert_eq!(json["steps"][0]["status"], "pending");

    server.observer.complete_run("success");
    {
        let state = server.observer.shared_state();
        let mut state = state.lock().expect("dashboard state lock");
        state.finished_at = Some(Instant::now() - Duration::from_secs(61));
    }
    server.wait().expect("dashboard should shut down");
}

fn get_body(port: u16, path: &str) -> String {
    for _ in 0..20 {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            stream
                .write_all(
                    format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                )
                .expect("write request");
            let mut response = String::new();
            stream.read_to_string(&mut response).expect("read response");
            return response
                .split("\r\n\r\n")
                .nth(1)
                .expect("http body")
                .to_string();
        }
        thread::sleep(Duration::from_millis(50));
    }

    panic!("dashboard server did not accept connections");
}
