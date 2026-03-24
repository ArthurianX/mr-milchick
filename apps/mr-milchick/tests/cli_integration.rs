#[path = "../../../crates/milchick-connectors/tests/support/mock_server.rs"]
mod mock_server;

use std::process::{Command, Output};

use mock_server::{MERGE_REQUEST_IID, MockGitLabServer, PROJECT_ID, base_env, borrow_env};

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
fn refine_mode_runs_end_to_end_through_runtime_wiring() {
    let server = MockGitLabServer::start();
    let envs = base_env(&server);
    let output = run_cli("refine", &borrow_env(&envs));

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
