use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Command, Output};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{Value, json};

const PROJECT_ID: &str = "412";
const MERGE_REQUEST_IID: &str = "3995";

#[derive(Debug, Clone)]
struct RequestRecord {
    method: String,
    path: String,
    body: String,
}

#[derive(Debug, Clone)]
struct MockNote {
    id: u64,
    body: String,
}

#[derive(Debug)]
struct ServerState {
    reviewers: Vec<String>,
    notes: Vec<MockNote>,
    request_log: Vec<RequestRecord>,
    next_note_id: u64,
}

struct MockGitLabServer {
    address: SocketAddr,
    running: Arc<AtomicBool>,
    state: Arc<Mutex<ServerState>>,
    handle: Option<JoinHandle<()>>,
}

impl MockGitLabServer {
    fn start() -> Self {
        Self::start_with_reviewers(Vec::new())
    }

    fn start_with_reviewers(reviewers: Vec<&str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock server should bind");
        listener
            .set_nonblocking(true)
            .expect("mock server should be nonblocking");
        let address = listener
            .local_addr()
            .expect("mock server should have address");
        let running = Arc::new(AtomicBool::new(true));
        let state = Arc::new(Mutex::new(ServerState {
            reviewers: reviewers
                .into_iter()
                .map(|value| value.to_string())
                .collect(),
            notes: Vec::new(),
            request_log: Vec::new(),
            next_note_id: 1,
        }));

        let handle = {
            let running = Arc::clone(&running);
            let state = Arc::clone(&state);

            thread::spawn(move || {
                while running.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => handle_connection(&mut stream, &state),
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(err) => panic!("mock server accept failed: {err}"),
                    }
                }
            })
        };

        Self {
            address,
            running,
            state,
            handle: Some(handle),
        }
    }

    fn api_base_url(&self) -> String {
        format!("http://{}/api/v4", self.address)
    }

    fn slack_webhook_url(&self) -> String {
        format!("http://{}/slack/workflow", self.address)
    }

    fn request_count(&self, method: &str, path: &str) -> usize {
        self.state
            .lock()
            .expect("state lock should succeed")
            .request_log
            .iter()
            .filter(|record| record.method == method && record.path == path)
            .count()
    }

    fn request_count_prefix(&self, method: &str, path_prefix: &str) -> usize {
        self.state
            .lock()
            .expect("state lock should succeed")
            .request_log
            .iter()
            .filter(|record| record.method == method && record.path.starts_with(path_prefix))
            .count()
    }

    fn note_bodies(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("state lock should succeed")
            .notes
            .iter()
            .map(|note| note.body.clone())
            .collect()
    }

    fn assigned_reviewers(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("state lock should succeed")
            .reviewers
            .clone()
    }

    fn request_bodies(&self, method: &str, path: &str) -> Vec<String> {
        self.state
            .lock()
            .expect("state lock should succeed")
            .request_log
            .iter()
            .filter(|record| record.method == method && record.path == path)
            .map(|record| record.body.clone())
            .collect()
    }
}

impl Drop for MockGitLabServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);

        if let Some(handle) = self.handle.take() {
            handle.join().expect("mock server should join cleanly");
        }
    }
}

fn handle_connection(stream: &mut TcpStream, state: &Arc<Mutex<ServerState>>) {
    stream
        .set_nonblocking(false)
        .expect("accepted stream should be switched to blocking mode");

    let request = read_http_request(stream);
    let mut guard = state.lock().expect("state lock should succeed");
    guard.request_log.push(RequestRecord {
        method: request.method.clone(),
        path: request.path.clone(),
        body: request.body.clone(),
    });

    let response = route_request(&request, &mut guard);
    write_http_response(stream, response.status_code, &response.body);
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    body: String,
}

#[derive(Debug)]
struct HttpResponse {
    status_code: u16,
    body: String,
}

fn read_http_request(stream: &mut TcpStream) -> HttpRequest {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("stream timeout should be set");

    let mut buffer = Vec::new();
    let mut temp = [0_u8; 1024];

    loop {
        let read = read_from_stream(stream, &mut temp, "request read should succeed");
        if read == 0 {
            break;
        }

        buffer.extend_from_slice(&temp[..read]);

        if let Some(header_end) = find_header_end(&buffer) {
            let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    if name.eq_ignore_ascii_case("content-length") {
                        value.trim().parse::<usize>().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            let total_length = header_end + 4 + content_length;
            while buffer.len() < total_length {
                let read = read_from_stream(stream, &mut temp, "request body read should succeed");
                if read == 0 {
                    break;
                }
                buffer.extend_from_slice(&temp[..read]);
            }

            let request_line = headers
                .lines()
                .next()
                .expect("request should contain a request line");
            let mut parts = request_line.split_whitespace();
            let method = parts
                .next()
                .expect("request should have method")
                .to_string();
            let path = parts.next().expect("request should have path").to_string();
            let body =
                String::from_utf8_lossy(&buffer[header_end + 4..header_end + 4 + content_length])
                    .to_string();

            return HttpRequest { method, path, body };
        }
    }

    panic!("incomplete HTTP request received by mock server");
}

fn read_from_stream(stream: &mut TcpStream, buffer: &mut [u8], error_message: &str) -> usize {
    loop {
        match stream.read(buffer) {
            Ok(read) => return read,
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => panic!("{error_message}: {err}"),
        }
    }
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_http_response(stream: &mut TcpStream, status_code: u16, body: &str) {
    let status_text = match status_code {
        200 => "OK",
        201 => "Created",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };

    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_code,
        status_text,
        body.len(),
        body
    );

    stream
        .write_all(response.as_bytes())
        .expect("response write should succeed");
}

fn route_request(request: &HttpRequest, state: &mut ServerState) -> HttpResponse {
    let mr_path = format!("/api/v4/projects/{PROJECT_ID}/merge_requests/{MERGE_REQUEST_IID}");
    let notes_path = format!("{mr_path}/notes");
    let changes_path = format!("{mr_path}/changes");
    let slack_workflow_path = "/slack/workflow";

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", path) if path == mr_path => HttpResponse {
            status_code: 200,
            body: json!({
                "iid": 3995,
                "title": "Frontend adjustments",
                "description": "Refresh the UI flow",
                "state": "opened",
                "draft": false,
                "web_url": "https://gitlab.example.com/group/project/-/merge_requests/3995",
                "author": {"username": "arthur"},
                "reviewers": state.reviewers.iter().map(|username| json!({"username": username})).collect::<Vec<_>>(),
            })
            .to_string(),
        },
        ("GET", path) if path == changes_path => HttpResponse {
            status_code: 200,
            body: json!({
                "changes": [
                    {
                        "old_path": "apps/frontend/button_old.tsx",
                        "new_path": "apps/frontend/button.tsx",
                        "new_file": false,
                        "renamed_file": false,
                        "deleted_file": false
                    }
                ]
            })
            .to_string(),
        },
        ("GET", path) if path == notes_path => HttpResponse {
            status_code: 200,
            body: json!(
                state
                    .notes
                    .iter()
                    .map(|note| json!({"id": note.id, "body": note.body}))
                    .collect::<Vec<_>>()
            )
            .to_string(),
        },
        ("GET", path) if path.starts_with("/api/v4/users?username=") => {
            let username = path
                .split_once("username=")
                .map(|(_, value)| value)
                .unwrap_or_default();
            let user = user_lookup(username);
            HttpResponse {
                status_code: 200,
                body: json!([user]).to_string(),
            }
        }
        ("PUT", path) if path == mr_path => {
            let body: Value =
                serde_json::from_str(&request.body).expect("reviewer assignment should be JSON");
            let reviewer_ids = body["reviewer_ids"]
                .as_array()
                .expect("reviewer_ids should be an array");

            state.reviewers = reviewer_ids
                .iter()
                .map(|value| {
                    let id = value.as_u64().expect("reviewer id should be a number");
                    username_for_id(id).to_string()
                })
                .collect();

            HttpResponse {
                status_code: 200,
                body: "{}".to_string(),
            }
        }
        ("POST", path) if path == notes_path => {
            let body: Value =
                serde_json::from_str(&request.body).expect("note creation should be JSON");
            let note_body = body["body"]
                .as_str()
                .expect("note body should be a string")
                .to_string();

            let note_id = state.next_note_id;
            state.next_note_id += 1;
            state.notes.push(MockNote {
                id: note_id,
                body: note_body,
            });

            HttpResponse {
                status_code: 201,
                body: json!({"id": note_id}).to_string(),
            }
        }
        ("PUT", path) if path.starts_with(&format!("{notes_path}/")) => {
            let note_id = path
                .rsplit('/')
                .next()
                .expect("note path should include id")
                .parse::<u64>()
                .expect("note id should be numeric");
            let body: Value =
                serde_json::from_str(&request.body).expect("note update should be JSON");
            let note_body = body["body"]
                .as_str()
                .expect("note body should be a string")
                .to_string();

            let note = state
                .notes
                .iter_mut()
                .find(|note| note.id == note_id)
                .expect("existing note should be present for update");
            note.body = note_body;

            HttpResponse {
                status_code: 200,
                body: json!({"id": note_id}).to_string(),
            }
        }
        ("POST", path) if path == slack_workflow_path => HttpResponse {
            status_code: 200,
            body: "{}".to_string(),
        },
        _ => HttpResponse {
            status_code: 404,
            body: json!({"error": "unmatched route", "path": request.path}).to_string(),
        },
    }
}

fn user_lookup(username: &str) -> Value {
    json!({
        "id": user_id_for_username(username),
        "username": username
    })
}

fn user_id_for_username(username: &str) -> u64 {
    match username {
        "principal-reviewer" => 1001,
        "bob" => 1002,
        "milchick-duty" => 1003,
        "alice" => 1004,
        other => panic!("unexpected user lookup for '{other}'"),
    }
}

fn username_for_id(id: u64) -> &'static str {
    match id {
        1001 => "principal-reviewer",
        1002 => "bob",
        1003 => "milchick-duty",
        1004 => "alice",
        other => panic!("unexpected reviewer id '{other}'"),
    }
}

fn run_cli(subcommand: &str, envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mr-milchick"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command.arg(subcommand);
    command.env_clear();

    for (key, value) in envs {
        command.env(key, value);
    }

    command.output().expect("CLI invocation should succeed")
}

fn base_env(server: &MockGitLabServer) -> Vec<(&'static str, String)> {
    vec![
        ("CI_PROJECT_ID", PROJECT_ID.to_string()),
        ("CI_MERGE_REQUEST_IID", MERGE_REQUEST_IID.to_string()),
        ("CI_PIPELINE_SOURCE", "merge_request_event".to_string()),
        (
            "CI_MERGE_REQUEST_SOURCE_BRANCH_NAME",
            "feat/intentional-cleanup".to_string(),
        ),
        (
            "CI_MERGE_REQUEST_TARGET_BRANCH_NAME",
            "develop".to_string(),
        ),
        ("CI_MERGE_REQUEST_LABELS", "".to_string()),
        ("GITLAB_TOKEN", "test-token".to_string()),
        ("GITLAB_BASE_URL", server.api_base_url()),
        (
            "MR_MILCHICK_REVIEWERS",
            r#"[{"username":"milchick-duty","fallback":true},{"username":"principal-reviewer","mandatory":true},{"username":"bob","areas":["frontend"]}]"#.to_string(),
        ),
        ("MR_MILCHICK_MAX_REVIEWERS", "2".to_string()),
        ("MR_MILCHICK_CODEOWNERS_ENABLED", "false".to_string()),
        ("MR_MILCHICK_SLACK_ENABLED", "false".to_string()),
        ("RUST_LOG", "off".to_string()),
    ]
}

fn borrow_env<'a>(envs: &'a [(&'static str, String)]) -> Vec<(&'a str, &'a str)> {
    envs.iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect()
}

#[test]
fn observe_mode_skips_gitlab_for_non_merge_request_pipelines() {
    let output = run_cli(
        "observe",
        &[
            ("CI_PROJECT_ID", PROJECT_ID),
            ("CI_PIPELINE_SOURCE", "push"),
            (
                "CI_MERGE_REQUEST_SOURCE_BRANCH_NAME",
                "feat/intentional-cleanup",
            ),
            ("CI_MERGE_REQUEST_TARGET_BRANCH_NAME", "develop"),
            ("CI_MERGE_REQUEST_LABELS", ""),
            ("RUST_LOG", "off"),
        ],
    );

    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("This pipeline does not currently present merge request responsibilities.")
    );
}

#[test]
fn observe_mode_reports_planned_actions_without_mutating_gitlab() {
    let server = MockGitLabServer::start();
    let envs = base_env(&server);
    let output = run_cli("observe", &borrow_env(&envs));

    assert!(
        output.status.success(),
        "command failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No findings were produced."));
    assert!(stdout.contains("If you run `refine`, it would:"));
    assert!(stdout.contains("[AssignReviewers] principal-reviewer, bob"));

    let mr_path = format!("/api/v4/projects/{PROJECT_ID}/merge_requests/{MERGE_REQUEST_IID}");
    let notes_path = format!("{mr_path}/notes");
    assert_eq!(server.request_count("GET", &mr_path), 1);
    assert_eq!(
        server.request_count("GET", &format!("{mr_path}/changes")),
        1
    );
    assert_eq!(server.request_count("POST", &notes_path), 0);
    assert_eq!(server.request_count("PUT", &mr_path), 0);
}

#[test]
fn refine_mode_is_idempotent_across_runs() {
    let server = MockGitLabServer::start();
    let envs = base_env(&server);
    let borrowed_envs = borrow_env(&envs);
    let mr_path = format!("/api/v4/projects/{PROJECT_ID}/merge_requests/{MERGE_REQUEST_IID}");
    let notes_path = format!("{mr_path}/notes");

    let first = run_cli("refine", &borrowed_envs);
    assert!(
        first.status.success(),
        "first refine failed: {}\n{}",
        String::from_utf8_lossy(&first.stderr),
        String::from_utf8_lossy(&first.stdout)
    );

    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(first_stdout.contains("Planned actions:"));
    assert!(first_stdout.contains("[AssignReviewers] principal-reviewer, bob"));
    assert!(first_stdout.contains("Execution report:"));
    assert!(first_stdout.contains("[ReviewersAssigned] principal-reviewer, bob"));
    assert!(first_stdout.contains("[CommentPosted] Mr. Milchick summary comment"));

    let notes = server.note_bodies();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("<!-- mr-milchick:summary -->"));
    assert_eq!(
        server.assigned_reviewers(),
        vec!["principal-reviewer".to_string(), "bob".to_string()]
    );

    let second = run_cli("refine", &borrowed_envs);
    assert!(
        second.status.success(),
        "second refine failed: {}\n{}",
        String::from_utf8_lossy(&second.stderr),
        String::from_utf8_lossy(&second.stdout)
    );

    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(second_stdout.contains("All recommended reviewers are already assigned."));
    assert!(second_stdout.contains("[CommentPosted] Mr. Milchick summary comment"));
    assert!(!second_stdout.contains("[ReviewersAssigned] principal-reviewer, bob"));

    let third = run_cli("refine", &borrowed_envs);
    assert!(
        third.status.success(),
        "third refine failed: {}\n{}",
        String::from_utf8_lossy(&third.stderr),
        String::from_utf8_lossy(&third.stdout)
    );

    let third_stdout = String::from_utf8_lossy(&third.stdout);
    assert!(third_stdout.contains("All recommended reviewers are already assigned."));
    assert!(third_stdout.contains("[CommentSkippedAlreadyPresent] Mr. Milchick summary comment"));
    assert!(!third_stdout.contains("[ReviewersAssigned] principal-reviewer, bob"));

    assert_eq!(server.request_count("PUT", &mr_path), 1);
    assert_eq!(server.request_count("POST", &notes_path), 1);
    assert_eq!(server.request_count("PUT", &format!("{notes_path}/1")), 1);
    assert_eq!(server.request_count("GET", &notes_path), 3);
    assert_eq!(
        server.request_count_prefix("GET", "/api/v4/users?username="),
        2
    );

    let reviewer_assignment_bodies = server.request_bodies("PUT", &mr_path);
    assert_eq!(reviewer_assignment_bodies.len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&reviewer_assignment_bodies[0])
            .expect("reviewer assignment body should parse"),
        json!({"reviewer_ids": [1001, 1002]})
    );
    assert_eq!(server.note_bodies().len(), 1);
    assert!(server.note_bodies()[0].contains("All recommended reviewers are already assigned."));
}

#[test]
fn refine_mode_merges_existing_reviewers_instead_of_replacing_them() {
    let server = MockGitLabServer::start_with_reviewers(vec!["alice"]);
    let envs = base_env(&server);
    let borrowed_envs = borrow_env(&envs);
    let mr_path = format!("/api/v4/projects/{PROJECT_ID}/merge_requests/{MERGE_REQUEST_IID}");

    let output = run_cli("refine", &borrowed_envs);
    assert!(
        output.status.success(),
        "refine failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[AssignReviewers] principal-reviewer, bob"));
    assert!(stdout.contains("[ReviewersAssigned] alice, principal-reviewer, bob"));

    assert_eq!(
        server.assigned_reviewers(),
        vec![
            "alice".to_string(),
            "principal-reviewer".to_string(),
            "bob".to_string(),
        ]
    );

    let reviewer_assignment_bodies = server.request_bodies("PUT", &mr_path);
    assert_eq!(reviewer_assignment_bodies.len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&reviewer_assignment_bodies[0])
            .expect("reviewer assignment body should parse"),
        json!({"reviewer_ids": [1004, 1001, 1002]})
    );
}

#[test]
fn refine_mode_posts_compact_slack_message_and_thread_payload() {
    let server = MockGitLabServer::start();
    let mut envs = base_env(&server);
    envs.retain(|(key, _)| *key != "MR_MILCHICK_SLACK_ENABLED");
    envs.push(("MR_MILCHICK_SLACK_WEBHOOK_URL", server.slack_webhook_url()));
    envs.push(("MR_MILCHICK_SLACK_CHANNEL", "C0ALY38CW3X".to_string()));

    let output = run_cli("refine", &borrow_env(&envs));
    assert!(
        output.status.success(),
        "refine failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let bodies = server.request_bodies("POST", "/slack/workflow");
    assert_eq!(bodies.len(), 1);

    let payload: Value =
        serde_json::from_str(&bodies[0]).expect("Slack workflow payload should parse");
    assert_eq!(payload["mr_milchick_talks_to"], json!("C0ALY38CW3X"));
    assert_eq!(
        payload["mr_milchick_says"],
        json!(
            ":gitlab: :noted2: Reviews Needed : https://gitlab.example.com/group/project/-/merge_requests/3995 > :thread:"
        )
    );

    let thread_message = payload["mr_milchick_says_thread"]
        .as_str()
        .expect("thread message should be a string");
    assert!(thread_message.contains("Review requested for:"));
    assert!(thread_message.contains("Frontend adjustments"));
    assert!(
        thread_message.contains("https://gitlab.example.com/group/project/-/merge_requests/3995")
    );
    assert!(thread_message.contains("Assigned reviewers: principal-reviewer, bob."));
}
