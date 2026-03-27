#[path = "support/mock_server.rs"]
mod mock_server;

use std::process::{Command, Output};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use mock_server::{MERGE_REQUEST_IID, MockGitLabServer, PROJECT_ID, base_env, borrow_env};

fn run_cli(subcommand: &str, extra_args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mr-milchick"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command.arg(subcommand);
    command.args(extra_args);
    command.env_clear();

    for (key, value) in envs {
        command.env(key, value);
    }

    command.output().expect("CLI invocation should succeed")
}

fn write_temp_flavor(contents: &str) -> String {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("mr-milchick-flavor-{unique}.toml"));
    fs::write(&path, contents).expect("temp flavor should be written");
    path.display().to_string()
}

#[test]
fn observe_mode_skips_gitlab_for_non_merge_request_pipelines() {
    let output = run_cli(
        "observe",
        &[],
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
    let output = run_cli("observe", &[], &borrow_env(&envs));

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
fn refine_mode_runs_end_to_end_through_runtime_wiring() {
    let server = MockGitLabServer::start();
    let envs = base_env(&server);
    let output = run_cli("refine", &[], &borrow_env(&envs));

    assert!(
        output.status.success(),
        "refine failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Planned actions:"));
    assert!(stdout.contains("[AssignReviewers] principal-reviewer, bob"));
    assert!(stdout.contains("Execution report:"));
    assert!(stdout.contains("[ReviewersAssigned] principal-reviewer, bob"));
    assert!(stdout.contains("[CommentPosted] Mr. Milchick summary comment"));

    assert_eq!(
        server.assigned_reviewers(),
        vec!["principal-reviewer".to_string(), "bob".to_string()]
    );
    assert_eq!(server.note_bodies().len(), 1);
}

#[test]
fn refine_mode_always_policy_sends_summary_notifications_even_when_summary_is_unchanged() {
    let server = MockGitLabServer::start();
    let mut envs = base_env(&server);
    envs.push(("MR_MILCHICK_REVIEWERS", "[]".to_string()));
    envs.push(("MR_MILCHICK_SLACK_ENABLED", "true".to_string()));
    envs.push(("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test".to_string()));
    envs.push(("MR_MILCHICK_SLACK_CHANNEL", "C0ALY38CW3X".to_string()));
    envs.push(("MR_MILCHICK_SLACK_BASE_URL", server.slack_api_base_url()));

    let first = run_cli("refine", &[], &borrow_env(&envs));
    assert!(
        first.status.success(),
        "first refine failed: {}\n{}",
        String::from_utf8_lossy(&first.stderr),
        String::from_utf8_lossy(&first.stdout)
    );

    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(first_stdout.contains("[CommentPosted] Mr. Milchick summary comment"));
    assert!(first_stdout.contains("[Notification SlackApp] delivered=true sent"));

    let second = run_cli("refine", &[], &borrow_env(&envs));
    assert!(
        second.status.success(),
        "second refine failed: {}\n{}",
        String::from_utf8_lossy(&second.stderr),
        String::from_utf8_lossy(&second.stdout)
    );

    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(second_stdout.contains("[CommentSkippedAlreadyPresent] Mr. Milchick summary comment"));
    assert!(second_stdout.contains("[Notification SlackApp] delivered=true sent"));

    let mr_path = format!("/api/v4/projects/{PROJECT_ID}/merge_requests/{MERGE_REQUEST_IID}");
    let notes_path = format!("{mr_path}/notes");
    assert_eq!(server.request_count("POST", &notes_path), 1);
    assert_eq!(server.note_bodies().len(), 1);
    assert_eq!(
        server.request_count("POST", "/slack/api/chat.postMessage"),
        4
    );
}

#[test]
fn refine_mode_on_applied_action_policy_skips_notifications_when_summary_is_unchanged() {
    let server = MockGitLabServer::start();
    let mut envs = base_env(&server);
    envs.push(("MR_MILCHICK_REVIEWERS", "[]".to_string()));
    envs.push(("MR_MILCHICK_SLACK_ENABLED", "true".to_string()));
    envs.push(("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test".to_string()));
    envs.push(("MR_MILCHICK_SLACK_CHANNEL", "C0ALY38CW3X".to_string()));
    envs.push(("MR_MILCHICK_SLACK_BASE_URL", server.slack_api_base_url()));
    envs.push((
        "MR_MILCHICK_NOTIFICATION_POLICY",
        "on-applied-action".to_string(),
    ));

    let first = run_cli("refine", &[], &borrow_env(&envs));
    assert!(
        first.status.success(),
        "first refine failed: {}\n{}",
        String::from_utf8_lossy(&first.stderr),
        String::from_utf8_lossy(&first.stdout)
    );

    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(first_stdout.contains("[CommentPosted] Mr. Milchick summary comment"));
    assert!(first_stdout.contains("[Notification SlackApp] delivered=true sent"));

    let second = run_cli("refine", &[], &borrow_env(&envs));
    assert!(
        second.status.success(),
        "second refine failed: {}\n{}",
        String::from_utf8_lossy(&second.stderr),
        String::from_utf8_lossy(&second.stdout)
    );

    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(second_stdout.contains("[CommentSkippedAlreadyPresent] Mr. Milchick summary comment"));
    assert!(
        second_stdout
            .contains("[Notification SlackApp] delivered=false skipped because summary unchanged")
    );

    let mr_path = format!("/api/v4/projects/{PROJECT_ID}/merge_requests/{MERGE_REQUEST_IID}");
    let notes_path = format!("{mr_path}/notes");
    assert_eq!(server.request_count("POST", &notes_path), 1);
    assert_eq!(server.note_bodies().len(), 1);
    assert_eq!(
        server.request_count("POST", "/slack/api/chat.postMessage"),
        2
    );
}

#[test]
fn refine_mode_uses_partial_slack_template_override_from_flavor_file() {
    let server = MockGitLabServer::start();
    let flavor_path = write_temp_flavor(
        r#"
[review_platform]
kind = "gitlab"

[[notifications]]
kind = "slack-app"
enabled = true

[templates.slack_app]
root = "Template override for {{mr_ref}}"
"#,
    );

    let mut envs = base_env(&server);
    envs.push(("MR_MILCHICK_REVIEWERS", "[]".to_string()));
    envs.push(("MR_MILCHICK_SLACK_ENABLED", "true".to_string()));
    envs.push(("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test".to_string()));
    envs.push(("MR_MILCHICK_SLACK_CHANNEL", "C0ALY38CW3X".to_string()));
    envs.push(("MR_MILCHICK_SLACK_BASE_URL", server.slack_api_base_url()));
    envs.push(("MR_MILCHICK_FLAVOR_PATH", flavor_path.clone()));

    let output = run_cli("refine", &[], &borrow_env(&envs));
    assert!(
        output.status.success(),
        "refine failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let bodies = server.request_bodies("POST", "/slack/api/chat.postMessage");
    assert_eq!(bodies.len(), 2);
    assert!(bodies[0].contains("Template override for MR #3995"));
    assert!(bodies[1].contains("No findings were produced."));

    let _ = fs::remove_file(flavor_path);
}

#[test]
fn refine_mode_invalid_template_override_falls_back_to_default_output() {
    let server = MockGitLabServer::start();
    let flavor_path = write_temp_flavor(
        r#"
[review_platform]
kind = "gitlab"

[[notifications]]
kind = "slack-app"
enabled = true

[templates.slack_app]
root = "Template override for {{unknown_placeholder}}"
"#,
    );

    let mut envs = base_env(&server);
    envs.push(("MR_MILCHICK_REVIEWERS", "[]".to_string()));
    envs.push(("MR_MILCHICK_SLACK_ENABLED", "true".to_string()));
    envs.push(("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test".to_string()));
    envs.push(("MR_MILCHICK_SLACK_CHANNEL", "C0ALY38CW3X".to_string()));
    envs.push(("MR_MILCHICK_SLACK_BASE_URL", server.slack_api_base_url()));
    envs.push(("MR_MILCHICK_FLAVOR_PATH", flavor_path.clone()));

    let output = run_cli("refine", &[], &borrow_env(&envs));
    assert!(
        output.status.success(),
        "refine failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let bodies = server.request_bodies("POST", "/slack/api/chat.postMessage");
    assert_eq!(bodies.len(), 2);
    assert!(bodies[0].contains(":gitlab: Mr. Milchick updated <"));

    let _ = fs::remove_file(flavor_path);
}

#[test]
fn observe_mode_supports_fixture_without_ci_env_and_prints_notification_preview() {
    let output = run_cli(
        "observe",
        &["--fixture", "fixtures/review-request.toml"],
        &[("RUST_LOG", "off")],
    );

    assert!(
        output.status.success(),
        "observe fixture failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("If you run `refine`, it would:"));
    assert!(stdout.contains("Notification previews:"));
    assert!(stdout.contains("SlackApp"));
    assert!(stdout.contains("Reviews Needed for"));
}

#[test]
fn refine_mode_fixture_sends_slack_notifications_without_gitlab_env() {
    let server = MockGitLabServer::start();
    let slack_base_url = server.slack_api_base_url();
    let flavor_path = write_temp_flavor(
        r#"
[review_platform]
kind = "gitlab"

[[notifications]]
kind = "slack-app"
enabled = true
"#,
    );

    let output = run_cli(
        "refine",
        &[
            "--fixture",
            "fixtures/review-request.toml",
            "--send-notifications",
        ],
        &[
            ("MR_MILCHICK_FLAVOR_PATH", flavor_path.as_str()),
            ("MR_MILCHICK_SLACK_ENABLED", "true"),
            ("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test"),
            ("MR_MILCHICK_SLACK_CHANNEL", "C0ALY38CW3X"),
            ("MR_MILCHICK_SLACK_BASE_URL", slack_base_url.as_str()),
            ("RUST_LOG", "off"),
        ],
    );

    assert!(
        output.status.success(),
        "refine fixture failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[Notification SlackApp] delivered=true sent"));
    assert_eq!(
        server.request_count("POST", "/slack/api/chat.postMessage"),
        2
    );
    assert_eq!(
        server.request_count_prefix("GET", "/api/v4/projects/"),
        0,
        "fixture mode should not talk to GitLab"
    );

    let _ = fs::remove_file(flavor_path);
}

#[test]
fn send_notifications_requires_fixture() {
    let output = run_cli("refine", &["--send-notifications"], &[("RUST_LOG", "off")]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--send-notifications"));
}
