#[path = "support/mock_server.rs"]
mod mock_server;

use std::process::{Command, Output};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use mock_server::{MockGitLabServer, PROJECT_ID, borrow_env};
#[cfg(not(feature = "github"))]
use mock_server::{MERGE_REQUEST_IID, base_env};
#[cfg(feature = "github")]
use mock_server::github_base_env;

fn run_cli(subcommand: &str, extra_args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mr-milchick"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command.arg(subcommand);
    command.args(extra_args);
    command.env_clear();
    let has_flavor_override = envs
        .iter()
        .any(|(key, _)| *key == "MR_MILCHICK_FLAVOR_PATH");
    let default_flavor = (!has_flavor_override).then(|| {
        write_temp_flavor(&format!(
            r#"[review_platform]
kind = "{}"

[[notifications]]
kind = "slack-app"
enabled = true

[[notifications]]
kind = "slack-workflow"
enabled = true
"#,
            compiled_review_platform_kind()
        ))
    });

    for (key, value) in envs {
        command.env(key, value);
    }
    if let Some(path) = &default_flavor {
        command.env("MR_MILCHICK_FLAVOR_PATH", path);
    }

    let output = command.output().expect("CLI invocation should succeed");

    if let Some(path) = default_flavor {
        let _ = fs::remove_file(path);
    }

    output
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

fn compiled_review_platform_kind() -> &'static str {
    #[cfg(feature = "github")]
    {
        "github"
    }
    #[cfg(not(feature = "github"))]
    {
        "gitlab"
    }
}

fn review_env(server: &MockGitLabServer) -> Vec<(&'static str, String)> {
    #[cfg(feature = "github")]
    {
        github_base_env(server)
    }
    #[cfg(not(feature = "github"))]
    {
        base_env(server)
    }
}

fn review_path() -> String {
    #[cfg(feature = "github")]
    {
        format!("/api/github/repos/ArthurianX/mr-milchick/pulls/3995")
    }
    #[cfg(not(feature = "github"))]
    {
        format!("/api/v4/projects/{PROJECT_ID}/merge_requests/{MERGE_REQUEST_IID}")
    }
}

fn review_files_path() -> String {
    #[cfg(feature = "github")]
    {
        format!("{}/files", review_path())
    }
    #[cfg(not(feature = "github"))]
    {
        format!("{}/changes", review_path())
    }
}

fn review_comments_path() -> String {
    #[cfg(feature = "github")]
    {
        "/api/github/repos/ArthurianX/mr-milchick/issues/3995/comments".to_string()
    }
    #[cfg(not(feature = "github"))]
    {
        format!("{}/notes", review_path())
    }
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
    assert!(stdout.contains("This pipeline does not currently present review responsibilities."));
}

#[cfg(feature = "github")]
#[test]
fn observe_mode_skips_github_for_non_review_pipelines() {
    let output = run_cli(
        "observe",
        &[],
        &[
            ("GITHUB_ACTIONS", "true"),
            ("GITHUB_EVENT_NAME", "push"),
            ("GITHUB_REPOSITORY", "ArthurianX/mr-milchick"),
            ("RUST_LOG", "off"),
        ],
    );

    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("This pipeline does not currently present review responsibilities."));
}

#[test]
fn observe_mode_reports_planned_actions_without_mutating_gitlab() {
    let server = MockGitLabServer::start();
    let envs = review_env(&server);
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

    assert_eq!(server.request_count("GET", &review_path()), 1);
    assert_eq!(server.request_count_prefix("GET", &review_files_path()), 1);
    assert_eq!(server.request_count("POST", &review_comments_path()), 0);
}

#[cfg(feature = "github")]
#[test]
fn refine_mode_runs_end_to_end_through_github_runtime_wiring() {
    let server = MockGitLabServer::start();
    let envs = github_base_env(&server);
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
fn refine_mode_runs_end_to_end_through_runtime_wiring() {
    let server = MockGitLabServer::start();
    let envs = review_env(&server);
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
    let mut envs = review_env(&server);
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

    assert_eq!(server.request_count("POST", &review_comments_path()), 1);
    assert_eq!(server.note_bodies().len(), 1);
    assert_eq!(
        server.request_count("POST", "/slack/api/chat.postMessage"),
        4
    );
}

#[test]
fn refine_mode_on_applied_action_policy_skips_notifications_when_summary_is_unchanged() {
    let server = MockGitLabServer::start();
    let mut envs = review_env(&server);
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

    assert_eq!(server.request_count("POST", &review_comments_path()), 1);
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
        &r#"
[review_platform]
kind = "__PLATFORM__"

[[notifications]]
kind = "slack-app"
enabled = true

[templates.slack_app]
update_root = "Template override for {{mr_ref}}"
"#
        .replace("__PLATFORM__", compiled_review_platform_kind()),
    );

    let mut envs = review_env(&server);
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
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("Template override for"))
    );
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("No findings were produced."))
    );

    let _ = fs::remove_file(flavor_path);
}

#[test]
fn refine_mode_invalid_template_override_falls_back_to_default_output() {
    let server = MockGitLabServer::start();
    let flavor_path = write_temp_flavor(
        &r#"
[review_platform]
kind = "__PLATFORM__"

[[notifications]]
kind = "slack-app"
enabled = true

[templates.slack_app]
update_root = "Template override for {{unknown_placeholder}}"
"#
        .replace("__PLATFORM__", compiled_review_platform_kind()),
    );

    let mut envs = review_env(&server);
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
    assert!(bodies.iter().all(|body| !body.contains("unknown_placeholder")));

    let _ = fs::remove_file(flavor_path);
}

#[test]
fn observe_mode_supports_fixture_without_ci_env_and_prints_notification_preview() {
    let output = run_cli(
        "observe",
        &["--fixture", "fixtures/first-notification.toml"],
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
    assert!(stdout.contains("took a first look at"));
}

#[test]
fn observe_mode_fixture_variant_override_can_force_update_preview() {
    let output = run_cli(
        "observe",
        &[
            "--fixture",
            "fixtures/first-notification.toml",
            "--fixture-variant",
            "update",
        ],
        &[("RUST_LOG", "off")],
    );

    assert!(
        output.status.success(),
        "observe fixture failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Mr. Milchick - updates"));
    assert!(!stdout.contains("took a first look at"));
}

#[test]
fn refine_mode_fixture_sends_slack_notifications_without_gitlab_env() {
    let server = MockGitLabServer::start();
    let slack_base_url = server.slack_api_base_url();
    let flavor_path = write_temp_flavor(&format!(
        r#"
[review_platform]
kind = "{platform}"

[[notifications]]
kind = "slack-app"
enabled = true
"#,
        platform = compiled_review_platform_kind(),
    ));

    let output = run_cli(
        "refine",
        &[
            "--fixture",
            "fixtures/first-notification.toml",
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
    assert_eq!(
        server.request_count_prefix("GET", "/api/github/repos/"),
        0,
        "fixture mode should not talk to GitHub"
    );

    let _ = fs::remove_file(flavor_path);
}

#[test]
fn send_notifications_requires_fixture() {
    let output = run_cli("refine", &["--send-notifications"], &[("RUST_LOG", "off")]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--send-notifications"));
}

#[test]
fn fixture_variant_requires_fixture() {
    let output = run_cli(
        "observe",
        &["--fixture-variant", "first"],
        &[("RUST_LOG", "off")],
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--fixture-variant"));
}
