#![cfg(feature = "github")]

#[path = "support/mock_server.rs"]
mod mock_server;

use mock_server::{MockGitLabServer, github_base_env};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn temp_config_path() -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("mr-milchick-github-config-{}.toml", unique))
}

fn write_temp_config(server: &MockGitLabServer) -> PathBuf {
    let path = temp_config_path();
    let content = format!(
        r#"
[platform]
kind = "github"
base_url = "{}"

[[areas.definitions]]
key = "frontend"
paths = ["apps/frontend/**"]
risk = "medium"

[codeowners]
enabled = false

[inference]
enabled = false
"#,
        server.github_api_base_url()
    );
    fs::write(&path, content).expect("config should be written");
    path
}

fn run_cli_with_env(envs: &[(&str, &str)], config_path: &PathBuf) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mr-milchick"));
    cmd.env_clear();
    for (key, value) in envs {
        if matches!(
            *key,
            "GITHUB_API_BASE_URL"
                | "MR_MILCHICK_CODEOWNERS_ENABLED"
                | "MR_MILCHICK_MAX_REVIEWERS"
                | "MR_MILCHICK_REVIEWERS"
                | "MR_MILCHICK_SLACK_ENABLED"
        ) {
            continue;
        }
        cmd.env(key, value);
    }
    cmd.env("MR_MILCHICK_CONFIG_PATH", config_path);
    cmd.arg("refine");
    cmd.output().expect("CLI should run")
}

#[tokio::test]
async fn actions_403_integration_error_updates_owned_comment() {
    let server = MockGitLabServer::start();

    // Seed an owned comment by github-actions[bot]
    server.add_github_comment(
        1,
        format!(
            "{}\n\nOld Summary",
            mr_milchick::connectors::github::MR_MILCHICK_MARKER
        ),
        Some("github-actions[bot]".to_string()),
    );

    // Configure mock to return 403 for /user with specific message
    server.set_github_user_response(
        403,
        r#"{"message": "Resource not accessible by integration"}"#,
    );

    // Ensure posted comments are attributed to github-actions[bot] if created (though we expect update)
    server.set_github_post_comment_author(Some("github-actions[bot]"));

    let config_path = write_temp_config(&server);
    let envs = github_base_env(&server);
    let borrowed_envs: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (*k, v.as_str())).collect();

    let output = run_cli_with_env(&borrowed_envs, &config_path);

    assert!(
        output.status.success(),
        "CLI should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify comment was updated (PATCH) not duplicated (POST)
    let patch_count = server.request_count(
        "PATCH",
        &format!(
            "/api/github/repos/{}/issues/comments/1",
            mock_server::GITHUB_PROJECT_KEY
        ),
    );
    assert_eq!(patch_count, 1, "Should update existing owned comment");

    // Verify no duplicate POST
    let post_count = server.request_count(
        "POST",
        &format!(
            "/api/github/repos/{}/issues/{}/comments",
            mock_server::GITHUB_PROJECT_KEY,
            mock_server::PULL_REQUEST_NUMBER
        ),
    );
    assert_eq!(post_count, 0, "Should not create new comment");

    fs::remove_file(config_path).ok();
}

#[tokio::test]
async fn actions_403_integration_error_rerun_does_not_duplicate() {
    let server = MockGitLabServer::start();

    // Seed an owned comment by github-actions[bot]
    server.add_github_comment(
        1,
        format!(
            "{}\n\nOld Summary",
            mr_milchick::connectors::github::MR_MILCHICK_MARKER
        ),
        Some("github-actions[bot]".to_string()),
    );

    server.set_github_user_response(
        403,
        r#"{"message": "Resource not accessible by integration"}"#,
    );
    server.set_github_post_comment_author(Some("github-actions[bot]"));

    let config_path = write_temp_config(&server);
    let envs = github_base_env(&server);
    let borrowed_envs: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (*k, v.as_str())).collect();

    // First run
    let output1 = run_cli_with_env(&borrowed_envs, &config_path);
    assert!(output1.status.success());

    // Second run (rerun)
    let output2 = run_cli_with_env(&borrowed_envs, &config_path);
    assert!(output2.status.success());

    // Should still only have 1 PATCH total (or 0 if unchanged? The test says "refine succeeds and updates owned comment; rerun does not duplicate")
    // If the summary is identical, it might skip. But let's check for POSTs.
    let post_count = server.request_count(
        "POST",
        &format!(
            "/api/github/repos/{}/issues/{}/comments",
            mock_server::GITHUB_PROJECT_KEY,
            mock_server::PULL_REQUEST_NUMBER
        ),
    );
    assert_eq!(post_count, 0, "Should not create new comment on rerun");

    fs::remove_file(config_path).ok();
}

#[tokio::test]
async fn actions_200_user_updates_pat_comment() {
    let server = MockGitLabServer::start();

    // Seed an owned comment by test-user (PAT user)
    server.add_github_comment(
        1,
        format!(
            "{}\n\nOld Summary",
            mr_milchick::connectors::github::MR_MILCHICK_MARKER
        ),
        Some("test-user".to_string()),
    );

    // Default /user returns 200 test-user
    server.set_github_user_response(200, r#"{"login": "test-user"}"#);
    server.set_github_post_comment_author(Some("test-user"));

    let config_path = write_temp_config(&server);
    let envs = github_base_env(&server);
    let borrowed_envs: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (*k, v.as_str())).collect();

    let output = run_cli_with_env(&borrowed_envs, &config_path);
    assert!(output.status.success());

    // Should update the test-user comment
    let patch_count = server.request_count(
        "PATCH",
        &format!(
            "/api/github/repos/{}/issues/comments/1",
            mock_server::GITHUB_PROJECT_KEY
        ),
    );
    assert_eq!(patch_count, 1, "Should update existing PAT-owned comment");

    fs::remove_file(config_path).ok();
}

#[tokio::test]
async fn actions_403_unrelated_message_errors() {
    let server = MockGitLabServer::start();

    // Seed an owned comment to force ownership check
    server.add_github_comment(
        1,
        format!(
            "{}\n\nOld Summary",
            mr_milchick::connectors::github::MR_MILCHICK_MARKER
        ),
        Some("test-user".to_string()),
    );

    // Configure mock to return 403 with unrelated message
    server.set_github_user_response(403, r#"{"message": "API rate limit exceeded"}"#);

    let config_path = write_temp_config(&server);
    let envs = github_base_env(&server);
    let borrowed_envs: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (*k, v.as_str())).collect();

    let output = run_cli_with_env(&borrowed_envs, &config_path);

    // Should fail because /user returned 403 with unrelated message
    assert!(!output.status.success(), "CLI should fail on unrelated 403");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("API rate limit exceeded"),
        "stderr should contain API rate limit exceeded: {}",
        stderr
    );

    // Verify no comment mutations
    let patch_count = server.request_count(
        "PATCH",
        &format!(
            "/api/github/repos/{}/issues/comments/1",
            mock_server::GITHUB_PROJECT_KEY
        ),
    );
    assert_eq!(
        patch_count, 0,
        "Should not update comment on unrelated error"
    );

    let post_count = server.request_count(
        "POST",
        &format!(
            "/api/github/repos/{}/issues/{}/comments",
            mock_server::GITHUB_PROJECT_KEY,
            mock_server::PULL_REQUEST_NUMBER
        ),
    );
    assert_eq!(
        post_count, 0,
        "Should not create new comment on unrelated error"
    );

    fs::remove_file(config_path).ok();
}
