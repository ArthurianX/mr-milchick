#[path = "support/mock_server.rs"]
mod mock_server;

use std::process::{Command, Output};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(feature = "github")]
use mock_server::github_base_env;
#[cfg(not(feature = "github"))]
use mock_server::{MERGE_REQUEST_IID, base_env};
use mock_server::{MockGitLabServer, PROJECT_ID, borrow_env};

fn run_cli(subcommand: &str, extra_args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mr-milchick"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command.arg(subcommand);
    command.args(extra_args);
    command.env_clear();
    let config_override = env_value(envs, "MR_MILCHICK_CONFIG_PATH")
        .or_else(|| env_value(envs, "MR_MILCHICK_FLAVOR_PATH"));
    let default_config = config_override
        .is_none()
        .then(|| write_temp_config(&build_test_config(envs)));

    for (key, value) in envs {
        if is_config_managed_env(key) || *key == "MR_MILCHICK_FLAVOR_PATH" {
            continue;
        }
        command.env(key, value);
    }
    if let Some(path) = config_override {
        command.env("MR_MILCHICK_CONFIG_PATH", path);
    } else if let Some(path) = &default_config {
        command.env("MR_MILCHICK_CONFIG_PATH", path);
    }

    let output = command.output().expect("CLI invocation should succeed");

    if let Some(path) = default_config {
        let _ = fs::remove_file(path);
    }

    output
}

fn write_temp_config(contents: &str) -> String {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("mr-milchick-config-{unique}.toml"));
    fs::write(&path, contents).expect("temp config should be written");
    path.display().to_string()
}

fn build_test_config(envs: &[(&str, &str)]) -> String {
    let mut sections = vec![format!(
        "[platform]\nkind = \"{}\"\nbase_url = {}\n",
        compiled_platform_connector_kind(),
        toml_string(
            env_value(envs, "GITLAB_BASE_URL")
                .or_else(|| env_value(envs, "GITHUB_API_BASE_URL"))
                .unwrap_or(default_platform_base_url())
        )
    )];

    sections.push(format!(
        "[execution]\ndry_run = {}\nnotification_policy = {}\n",
        legacy_bool(env_value(envs, "MR_MILCHICK_DRY_RUN")).unwrap_or(false),
        toml_string(env_value(envs, "MR_MILCHICK_NOTIFICATION_POLICY").unwrap_or("always"))
    ));

    sections.push(build_reviewers_config(envs));
    sections.push(format!(
        "[codeowners]\nenabled = {}\n",
        legacy_bool(env_value(envs, "MR_MILCHICK_CODEOWNERS_ENABLED")).unwrap_or(true)
    ));

    if legacy_bool(env_value(envs, "MR_MILCHICK_SLACK_ENABLED")).unwrap_or(false) {
        sections.push(format!(
            "[notifications.slack_app]\nenabled = true\nchannel = {}\nbase_url = {}\n",
            toml_optional_string(env_value(envs, "MR_MILCHICK_SLACK_CHANNEL")),
            toml_string(
                env_value(envs, "MR_MILCHICK_SLACK_BASE_URL").unwrap_or("https://slack.com/api")
            )
        ));

        if let Some(user_map) = env_value(envs, "MR_MILCHICK_SLACK_USER_MAP") {
            sections.push(build_user_map_config(user_map));
        }
    }

    if legacy_bool(env_value(envs, "MR_MILCHICK_SLACK_ENABLED")).unwrap_or(false)
        && env_value(envs, "MR_MILCHICK_SLACK_WEBHOOK_URL").is_some()
    {
        sections.push(format!(
            "[notifications.slack_workflow]\nenabled = true\nchannel = {}\n",
            toml_optional_string(env_value(envs, "MR_MILCHICK_SLACK_CHANNEL"))
        ));
    }

    sections.join("\n")
}

fn build_reviewers_config(envs: &[(&str, &str)]) -> String {
    let max_reviewers = env_value(envs, "MR_MILCHICK_MAX_REVIEWERS").unwrap_or("2");
    let mut config = format!("[reviewers]\nmax_reviewers = {}\n", max_reviewers);

    let Some(raw) = env_value(envs, "MR_MILCHICK_REVIEWERS") else {
        return config;
    };

    let reviewers = serde_json::from_str::<Vec<serde_json::Value>>(raw)
        .expect("test reviewer config should be valid JSON");
    for reviewer in reviewers {
        let username = reviewer["username"]
            .as_str()
            .expect("reviewer username should be present");
        config.push_str("\n[[reviewers.definitions]]\n");
        config.push_str(&format!("username = {}\n", toml_string(username)));
        if let Some(areas) = reviewer["areas"].as_array() {
            let areas = areas
                .iter()
                .map(|area| toml_string(area.as_str().expect("area should be a string")))
                .collect::<Vec<_>>()
                .join(", ");
            config.push_str(&format!("areas = [{}]\n", areas));
        }
        if reviewer["fallback"].as_bool().unwrap_or(false) {
            config.push_str("fallback = true\n");
        }
        if reviewer["mandatory"].as_bool().unwrap_or(false) {
            config.push_str("mandatory = true\n");
        }
    }

    config
}

fn build_user_map_config(raw: &str) -> String {
    let user_map = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(raw)
        .expect("test Slack user map should be valid JSON");
    let mut config = String::from("[notifications.slack_app.user_map]\n");
    for (username, user_id) in user_map {
        config.push_str(&format!(
            "{} = {}\n",
            toml_string(&username),
            toml_string(user_id.as_str().expect("Slack user id should be a string"))
        ));
    }
    config
}

fn env_value<'a>(envs: &'a [(&str, &str)], key: &str) -> Option<&'a str> {
    envs.iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| *value)
}

fn is_config_managed_env(key: &str) -> bool {
    matches!(
        key,
        "MR_MILCHICK_REVIEWERS"
            | "MR_MILCHICK_MAX_REVIEWERS"
            | "MR_MILCHICK_CODEOWNERS_ENABLED"
            | "MR_MILCHICK_CODEOWNERS_PATH"
            | "MR_MILCHICK_DRY_RUN"
            | "MR_MILCHICK_NOTIFICATION_POLICY"
            | "MR_MILCHICK_LLM_ENABLED"
            | "MR_MILCHICK_LLM_MODEL_PATH"
            | "MR_MILCHICK_LLM_TIMEOUT_MS"
            | "MR_MILCHICK_LLM_MAX_PATCH_BYTES"
            | "MR_MILCHICK_LLM_CONTEXT_TOKENS"
            | "MR_MILCHICK_LLM_TRACE"
            | "MR_MILCHICK_SLACK_ENABLED"
            | "MR_MILCHICK_SLACK_CHANNEL"
            | "MR_MILCHICK_SLACK_BASE_URL"
            | "MR_MILCHICK_SLACK_USER_MAP"
            | "GITLAB_BASE_URL"
            | "GITHUB_API_BASE_URL"
            | "MR_MILCHICK_CONFIG_PATH"
    )
}

fn legacy_bool(value: Option<&str>) -> Option<bool> {
    value.map(|value| matches!(value, "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"))
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("string should serialize")
}

fn toml_optional_string(value: Option<&str>) -> String {
    value.map(toml_string).unwrap_or_else(|| "\"\"".to_string())
}

fn default_platform_base_url() -> &'static str {
    #[cfg(feature = "github")]
    {
        "https://api.github.com"
    }
    #[cfg(not(feature = "github"))]
    {
        "https://gitlab.com/api/v4"
    }
}

fn compiled_platform_connector_kind() -> &'static str {
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
    assert!(!stdout.contains("[Notification "));

    assert_eq!(
        server.assigned_reviewers(),
        vec!["principal-reviewer".to_string(), "bob".to_string()]
    );
    assert_eq!(server.note_bodies().len(), 1);
}

#[cfg(feature = "slack-app")]
#[test]
fn refine_mode_always_policy_sends_summary_notifications_even_when_summary_is_unchanged() {
    let server = MockGitLabServer::start();
    let config_path = write_temp_config(&format!(
        r#"[platform]
kind = "{platform}"
base_url = {base_url}

[execution]
notification_policy = "always"

[reviewers]
max_reviewers = 2

[codeowners]
enabled = false

[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
base_url = {slack_base_url}
"#,
        platform = compiled_platform_connector_kind(),
        base_url = toml_string(server.api_base_url().as_str()),
        slack_base_url = toml_string(server.slack_api_base_url().as_str()),
    ));
    let mut envs = review_env(&server);
    envs.push(("MR_MILCHICK_CONFIG_PATH", config_path.clone()));
    envs.push(("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test".to_string()));

    let first = run_cli("refine", &[], &borrow_env(&envs));
    assert!(
        first.status.success(),
        "first refine failed: {}\n{}",
        String::from_utf8_lossy(&first.stderr),
        String::from_utf8_lossy(&first.stdout)
    );

    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(
        first_stdout.contains("[CommentPosted] Mr. Milchick summary comment"),
        "stdout was:\n{first_stdout}"
    );
    assert!(
        first_stdout.contains("[Notification SlackApp] delivered=true sent"),
        "stdout was:\n{first_stdout}"
    );

    let second = run_cli("refine", &[], &borrow_env(&envs));
    assert!(
        second.status.success(),
        "second refine failed: {}\n{}",
        String::from_utf8_lossy(&second.stderr),
        String::from_utf8_lossy(&second.stdout)
    );

    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(
        second_stdout.contains("[CommentSkippedAlreadyPresent] Mr. Milchick summary comment"),
        "stdout was:\n{second_stdout}"
    );
    assert!(
        second_stdout.contains("[Notification SlackApp] delivered=true sent"),
        "stdout was:\n{second_stdout}"
    );

    assert_eq!(server.request_count("POST", &review_comments_path()), 1);
    assert_eq!(server.note_bodies().len(), 1);
    assert_eq!(
        server.request_count("POST", "/slack/api/chat.postMessage"),
        3
    );
    let _ = fs::remove_file(config_path);
}

#[cfg(feature = "slack-app")]
#[test]
fn refine_mode_on_applied_action_policy_skips_notifications_when_summary_is_unchanged() {
    let server = MockGitLabServer::start();
    let config_path = write_temp_config(&format!(
        r#"[platform]
kind = "{platform}"
base_url = {base_url}

[execution]
notification_policy = "on-applied-action"

[reviewers]
max_reviewers = 2

[codeowners]
enabled = false

[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
base_url = {slack_base_url}
"#,
        platform = compiled_platform_connector_kind(),
        base_url = toml_string(server.api_base_url().as_str()),
        slack_base_url = toml_string(server.slack_api_base_url().as_str()),
    ));
    let mut envs = review_env(&server);
    envs.push(("MR_MILCHICK_CONFIG_PATH", config_path.clone()));
    envs.push(("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test".to_string()));

    let first = run_cli("refine", &[], &borrow_env(&envs));
    assert!(
        first.status.success(),
        "first refine failed: {}\n{}",
        String::from_utf8_lossy(&first.stderr),
        String::from_utf8_lossy(&first.stdout)
    );

    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(
        first_stdout.contains("[CommentPosted] Mr. Milchick summary comment"),
        "stdout was:\n{first_stdout}"
    );
    assert!(
        first_stdout.contains("[Notification SlackApp] delivered=true sent"),
        "stdout was:\n{first_stdout}"
    );

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
    let _ = fs::remove_file(config_path);
}

#[cfg(feature = "slack-app")]
#[test]
fn refine_mode_uses_partial_slack_template_override_from_config_file() {
    let server = MockGitLabServer::start();
    let config_body = r#"
[platform]
kind = "__PLATFORM__"
base_url = "__BASE_URL__"

[reviewers]
max_reviewers = 2

[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
base_url = "__SLACK_BASE_URL__"

[templates.slack_app]
update_root = "Template override for {{mr_ref}}"
"#
    .replace("__PLATFORM__", compiled_platform_connector_kind())
    .replace("__BASE_URL__", &server.api_base_url())
    .replace("__SLACK_BASE_URL__", &server.slack_api_base_url());
    let config_path = write_temp_config(&config_body);

    let mut envs = review_env(&server);
    envs.push(("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test".to_string()));
    envs.push(("MR_MILCHICK_CONFIG_PATH", config_path.clone()));

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

    let _ = fs::remove_file(config_path);
}

#[cfg(feature = "slack-app")]
#[test]
fn refine_mode_invalid_template_override_falls_back_to_default_output() {
    let server = MockGitLabServer::start();
    let config_body = r#"
[platform]
kind = "__PLATFORM__"
base_url = "__BASE_URL__"

[reviewers]
max_reviewers = 2

[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
base_url = "__SLACK_BASE_URL__"

[templates.slack_app]
update_root = "Template override for {{unknown_placeholder}}"
"#
    .replace("__PLATFORM__", compiled_platform_connector_kind())
    .replace("__BASE_URL__", &server.api_base_url())
    .replace("__SLACK_BASE_URL__", &server.slack_api_base_url());
    let config_path = write_temp_config(&config_body);

    let mut envs = review_env(&server);
    envs.push(("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test".to_string()));
    envs.push(("MR_MILCHICK_CONFIG_PATH", config_path.clone()));

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
            .all(|body| !body.contains("unknown_placeholder"))
    );

    let _ = fs::remove_file(config_path);
}

#[test]
fn observe_mode_supports_fixture_without_ci_env_without_notification_sinks() {
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
    assert!(stdout.contains("No notification previews were produced."));
}

#[cfg(feature = "slack-app")]
#[test]
fn observe_mode_fixture_variant_override_can_force_update_preview() {
    let config_path = write_temp_config(&format!(
        r#"[platform]
kind = "{platform}"
base_url = {base_url}

[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
"#,
        platform = compiled_platform_connector_kind(),
        base_url = toml_string(default_platform_base_url()),
    ));
    let output = run_cli(
        "observe",
        &[
            "--fixture",
            "fixtures/first-notification.toml",
            "--fixture-variant",
            "update",
        ],
        &[
            ("MR_MILCHICK_CONFIG_PATH", config_path.as_str()),
            ("RUST_LOG", "off"),
        ],
    );

    assert!(
        output.status.success(),
        "observe fixture failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Notification previews:"));
    assert!(stdout.contains("Mr. Milchick - updates"));
    assert!(!stdout.contains("took a first look at"));

    let _ = fs::remove_file(config_path);
}

#[cfg(feature = "slack-app")]
#[test]
fn observe_mode_supports_fixture_without_ci_env_and_prints_notification_preview() {
    let config_path = write_temp_config(&format!(
        r#"[platform]
kind = "{platform}"
base_url = {base_url}

[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
"#,
        platform = compiled_platform_connector_kind(),
        base_url = toml_string(default_platform_base_url()),
    ));
    let output = run_cli(
        "observe",
        &["--fixture", "fixtures/first-notification.toml"],
        &[
            ("MR_MILCHICK_CONFIG_PATH", config_path.as_str()),
            ("RUST_LOG", "off"),
        ],
    );

    assert!(
        output.status.success(),
        "observe fixture failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Notification previews:"));
    assert!(stdout.contains("SlackApp"));
    assert!(stdout.contains("took a first look at"));

    let _ = fs::remove_file(config_path);
}

#[cfg(feature = "slack-app")]
#[test]
fn refine_mode_fixture_sends_slack_notifications_without_gitlab_env() {
    let server = MockGitLabServer::start();
    let slack_base_url = server.slack_api_base_url();
    let config_path = write_temp_config(&format!(
        r#"
[platform]
kind = "{platform}"
base_url = {base_url}

[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
base_url = {slack_base_url}
"#,
        platform = compiled_platform_connector_kind(),
        base_url = toml_string(default_platform_base_url()),
        slack_base_url = toml_string(slack_base_url.as_str()),
    ));

    let output = run_cli(
        "refine",
        &[
            "--fixture",
            "fixtures/first-notification.toml",
            "--send-notifications",
        ],
        &[
            ("MR_MILCHICK_CONFIG_PATH", config_path.as_str()),
            ("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test"),
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

    let _ = fs::remove_file(config_path);
}

#[cfg(feature = "slack-app")]
#[test]
fn refine_mode_fixture_update_reuses_existing_slack_thread_for_same_mr() {
    let server = MockGitLabServer::start();
    let slack_base_url = server.slack_api_base_url();
    let config_path = write_temp_config(&format!(
        r#"
[platform]
kind = "{platform}"
base_url = {base_url}

[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
base_url = {slack_base_url}
"#,
        platform = compiled_platform_connector_kind(),
        base_url = toml_string(default_platform_base_url()),
        slack_base_url = toml_string(slack_base_url.as_str()),
    ));

    let common_env = [
        ("MR_MILCHICK_CONFIG_PATH", config_path.as_str()),
        ("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test"),
        ("RUST_LOG", "off"),
    ];

    let first = run_cli(
        "refine",
        &[
            "--fixture",
            "fixtures/threaded-first-notification.toml",
            "--send-notifications",
        ],
        &common_env,
    );
    assert!(
        first.status.success(),
        "first threaded fixture failed: {}\n{}",
        String::from_utf8_lossy(&first.stderr),
        String::from_utf8_lossy(&first.stdout)
    );

    let update = run_cli(
        "refine",
        &[
            "--fixture",
            "fixtures/threaded-update-notification.toml",
            "--send-notifications",
        ],
        &common_env,
    );
    assert!(
        update.status.success(),
        "update threaded fixture failed: {}\n{}",
        String::from_utf8_lossy(&update.stderr),
        String::from_utf8_lossy(&update.stdout)
    );

    assert_eq!(
        server.request_count_prefix("GET", "/slack/api/conversations.history"),
        1
    );

    let bodies = server.request_bodies("POST", "/slack/api/chat.postMessage");
    assert_eq!(bodies.len(), 3);

    let update_payload: serde_json::Value =
        serde_json::from_str(&bodies[2]).expect("update Slack payload should parse");
    assert_eq!(
        update_payload["thread_ts"],
        serde_json::json!("1700000000.000001")
    );

    let update_text = update_payload["text"]
        .as_str()
        .expect("update text should be a string");
    assert!(update_text.contains("Mr. Milchick - updates on"));
    assert!(update_text.contains("MR #3997"));
    assert!(update_text.contains("Button spacing changed near the CTA"));

    let _ = fs::remove_file(config_path);
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
