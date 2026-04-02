#[cfg(feature = "llm-local")]
mod llm_local_smoke {
    use std::env;
    use std::num::NonZeroU32;
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::time::Duration;

    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::list_llama_ggml_backend_devices;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::model::LlamaModel;
    use llama_cpp_2::model::params::{LlamaModelParams, LlamaSplitMode};
    use mr_milchick::core::inference::{
        LocalLlamaReviewInferenceEngine, RecommendationCategory, ReviewInferenceOutcome,
        ReviewInferenceStatus, analyze_with_timeout,
    };
    use mr_milchick::core::model::{
        Actor, ChangeType, ChangedFile, RepositoryRef, ReviewMetadata, ReviewPlatformKind,
        ReviewRef, ReviewSnapshot,
    };

    const DEFAULT_SMOKE_TIMEOUT_MS: u64 = 120_000;
    const DEFAULT_SMOKE_PATCH_BUDGET: usize = 4_096;
    const DEFAULT_SMOKE_CONTEXT_TOKENS: usize = 4_096;

    static SMOKE_TEST_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

    fn model_path_from_env() -> PathBuf {
        PathBuf::from(
            env::var("MR_MILCHICK_LLM_MODEL_PATH")
                .expect("set MR_MILCHICK_LLM_MODEL_PATH to a local GGUF file"),
        )
    }

    fn smoke_timeout() -> Duration {
        Duration::from_millis(
            env::var("MR_MILCHICK_LLM_SMOKE_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(DEFAULT_SMOKE_TIMEOUT_MS),
        )
    }

    fn smoke_patch_budget() -> usize {
        env::var("MR_MILCHICK_LLM_SMOKE_MAX_PATCH_BYTES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_SMOKE_PATCH_BUDGET)
    }

    fn smoke_context_tokens() -> usize {
        env::var("MR_MILCHICK_LLM_SMOKE_CONTEXT_TOKENS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_SMOKE_CONTEXT_TOKENS)
    }

    fn local_engine() -> LocalLlamaReviewInferenceEngine {
        LocalLlamaReviewInferenceEngine::new(
            model_path_from_env(),
            smoke_patch_budget(),
            smoke_context_tokens(),
        )
            .expect("local smoke test engine should initialize from env")
    }

    fn smoke_test_lock() -> &'static tokio::sync::Mutex<()> {
        SMOKE_TEST_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    fn sample_snapshot(
        title: &str,
        description: &str,
        labels: &[&str],
        changed_files: Vec<ChangedFile>,
    ) -> ReviewSnapshot {
        ReviewSnapshot {
            review_ref: ReviewRef {
                platform: ReviewPlatformKind::GitHub,
                project_key: "ArthurianX/mr-milchick".to_string(),
                review_id: "3995".to_string(),
                web_url: Some("https://example.test/pull/3995".to_string()),
            },
            repository: RepositoryRef {
                platform: ReviewPlatformKind::GitHub,
                namespace: "ArthurianX".to_string(),
                name: "mr-milchick".to_string(),
                web_url: Some("https://example.test/ArthurianX/mr-milchick".to_string()),
            },
            title: title.to_string(),
            description: Some(description.to_string()),
            author: Actor {
                username: "arthur".to_string(),
                display_name: Some("Arthur".to_string()),
            },
            participants: Vec::new(),
            changed_files,
            labels: labels.iter().map(|label| (*label).to_string()).collect(),
            is_draft: false,
            default_branch: Some("main".to_string()),
            metadata: ReviewMetadata {
                source_branch: Some("feat/local-llm-smoke".to_string()),
                target_branch: Some("main".to_string()),
                commit_count: Some(2),
                approvals_required: Some(1),
                approvals_given: Some(0),
            },
        }
    }

    fn added_line_count(patch: &str) -> u32 {
        patch
            .lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .count() as u32
    }

    fn deleted_line_count(patch: &str) -> u32 {
        patch
            .lines()
            .filter(|line| line.starts_with('-') && !line.starts_with("---"))
            .count() as u32
    }

    fn changed_file(path: &str, patch: &str) -> ChangedFile {
        ChangedFile {
            path: path.to_string(),
            previous_path: None,
            change_type: ChangeType::Modified,
            additions: Some(added_line_count(patch)),
            deletions: Some(deleted_line_count(patch)),
            patch: Some(patch.to_string()),
        }
    }

    fn deleted_file(path: &str, patch: &str) -> ChangedFile {
        ChangedFile {
            path: path.to_string(),
            previous_path: None,
            change_type: ChangeType::Deleted,
            additions: Some(0),
            deletions: Some(deleted_line_count(patch)),
            patch: Some(patch.to_string()),
        }
    }

    fn report_output(case_name: &str, outcome: &ReviewInferenceOutcome) {
        eprintln!("case={case_name} status={:?}", outcome.status);
        if let Some(detail) = &outcome.detail {
            eprintln!("case={case_name} detail={detail}");
        }
        if let Some(summary) = &outcome.insights.summary {
            eprintln!("case={case_name} summary={summary}");
        }
        for recommendation in &outcome.insights.recommendations {
            eprintln!(
                "case={case_name} recommendation[{:?}]={}",
                recommendation.category, recommendation.message
            );
        }
    }

    fn combined_output_text(outcome: &ReviewInferenceOutcome) -> String {
        let mut output = String::new();
        if let Some(summary) = &outcome.insights.summary {
            output.push_str(summary);
            output.push('\n');
        }
        for recommendation in &outcome.insights.recommendations {
            output.push_str(&recommendation.message);
            output.push('\n');
        }
        output.to_ascii_lowercase()
    }

    fn assert_contains_any_keyword(output: &str, keywords: &[&str]) {
        assert!(
            keywords.iter().any(|keyword| output.contains(keyword)),
            "expected output to contain one of {:?}, got:\n{}",
            keywords,
            output
        );
    }

    fn assert_contains_keyword_group(output: &str, groups: &[&[&str]]) {
        for group in groups {
            assert!(
                group.iter().any(|keyword| output.contains(keyword)),
                "expected output to contain one of {:?}, got:\n{}",
                group,
                output
            );
        }
    }

    fn normalized_quality_text(text: &str) -> String {
        text.chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase()
    }

    fn assert_no_duplicate_recommendations(outcome: &ReviewInferenceOutcome, case_name: &str) {
        let mut seen = std::collections::BTreeSet::new();
        for recommendation in &outcome.insights.recommendations {
            let normalized = normalized_quality_text(&recommendation.message);
            assert!(
                seen.insert(normalized),
                "expected unique recommendations for {case_name}, got duplicate message:\n{}",
                recommendation.message
            );
        }
    }

    fn assert_no_prompt_parroting(output: &str, case_name: &str) {
        for forbidden in [
            "potential review cues from the diff",
            "the first character of your reply must be",
            "if the snapshot includes deleted tests",
        ] {
            assert!(
                !output.contains(forbidden),
                "expected {case_name} output to avoid parroting prompt instruction '{forbidden}', got:\n{output}"
            );
        }
    }

    fn assert_required_categories(
        outcome: &ReviewInferenceOutcome,
        case_name: &str,
        required_categories: &[RecommendationCategory],
    ) {
        for category in required_categories {
            assert!(
                outcome
                    .insights
                    .recommendations
                    .iter()
                    .any(|recommendation| recommendation.category == *category),
                "expected {case_name} to include category {:?}, got {:?}",
                category,
                outcome
                    .insights
                    .recommendations
                    .iter()
                    .map(|recommendation| recommendation.category)
                    .collect::<Vec<_>>()
            );
        }
    }

    async fn run_reaction_case(
        case_name: &str,
        snapshot: ReviewSnapshot,
        expected_keywords: &[&str],
        expected_keyword_groups: &[&[&str]],
        required_categories: &[RecommendationCategory],
        minimum_recommendations: usize,
    ) {
        let _guard = smoke_test_lock().lock().await;
        let engine = local_engine();
        let outcome = analyze_with_timeout(&engine, &snapshot, smoke_timeout()).await;
        report_output(case_name, &outcome);

        assert_eq!(
            outcome.status,
            ReviewInferenceStatus::Ready,
            "expected llama.cpp to return structured recommendations for {case_name}"
        );
        assert!(
            !outcome.insights.is_empty(),
            "expected non-empty insights for {case_name}"
        );

        let output = combined_output_text(&outcome);
        assert_contains_any_keyword(&output, expected_keywords);
        assert_contains_keyword_group(&output, expected_keyword_groups);
        assert_no_prompt_parroting(&output, case_name);
        assert_no_duplicate_recommendations(&outcome, case_name);
        assert_required_categories(&outcome, case_name, required_categories);
        assert!(
            outcome.insights.summary.is_some(),
            "expected non-empty summary for {case_name}"
        );
        assert!(
            outcome.insights.recommendations.len() >= minimum_recommendations,
            "expected at least {} recommendations for {}, got {:?}",
            minimum_recommendations,
            case_name,
            outcome.insights.recommendations
        );
    }

    fn probe_contexts(
        backend: &LlamaBackend,
        model: &LlamaModel,
        scenario: &str,
        attempts: Vec<(&str, LlamaContextParams)>,
        successful_attempts: &mut Vec<String>,
        failed_attempts: &mut Vec<String>,
    ) {
        for (label, params) in attempts {
            let attempt_label = format!("{scenario}:{label}");
            match model.new_context(backend, params) {
                Ok(context) => {
                    eprintln!("context[{attempt_label}]=ok n_ctx={}", context.n_ctx());
                    successful_attempts.push(attempt_label);
                    drop(context);
                }
                Err(err) => {
                    eprintln!("context[{attempt_label}]=err {err}");
                    failed_attempts.push(format!("{attempt_label}: {err}"));
                }
            }
        }
    }

    #[test]
    #[ignore = "requires MR_MILCHICK_LLM_MODEL_PATH to point at a local GGUF"]
    fn backend_load_and_context_probe() {
        let _guard = smoke_test_lock().blocking_lock();
        let model_path = model_path_from_env();
        assert!(
            model_path.is_file(),
            "MR_MILCHICK_LLM_MODEL_PATH must point to an existing file, got '{}'",
            model_path.display()
        );

        eprintln!("model_path={}", model_path.display());

        let backend = LlamaBackend::init().expect("llama backend should initialize");
        eprintln!("backend_init=ok");
        let backend_devices = list_llama_ggml_backend_devices();
        let cpu_device_index = backend_devices
            .iter()
            .find(|device| matches!(device.device_type, llama_cpp_2::LlamaBackendDeviceType::Cpu))
            .map(|device| device.index);

        for device in backend_devices {
            eprintln!(
                "backend_device[index={} type={:?} backend={} name={} desc={} total={} free={}]",
                device.index,
                device.device_type,
                device.backend,
                device.name,
                device.description,
                device.memory_total,
                device.memory_free
            );
        }

        let model_params = LlamaModelParams::default().with_n_gpu_layers(0);
        let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)
            .expect("model should load from GGUF");

        eprintln!("model_load=ok");
        eprintln!("model_n_ctx_train={}", model.n_ctx_train());
        eprintln!("model_n_layer={}", model.n_layer());
        eprintln!("model_n_head={}", model.n_head());
        eprintln!("model_n_head_kv={}", model.n_head_kv());
        eprintln!("model_is_recurrent={}", model.is_recurrent());
        eprintln!("model_is_hybrid={}", model.is_hybrid());
        eprintln!("model_size_bytes={}", model.size());
        eprintln!("model_n_params={}", model.n_params());
        for key in [
            "general.architecture",
            "general.name",
            "tokenizer.ggml.model",
            "tokenizer.chat_template",
        ] {
            match model.meta_val_str(key) {
                Ok(value) => eprintln!("meta[{key}]={value}"),
                Err(err) => eprintln!("meta[{key}]=<unavailable: {err}>"),
            }
        }

        let thread_count = std::thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(4)
            .min(i32::MAX as usize) as i32;
        eprintln!("thread_count={thread_count}");

        let attempts = vec![
            ("default", LlamaContextParams::default()),
            (
                "default-cpu-only",
                LlamaContextParams::default().with_op_offload(false),
            ),
            (
                "ctx-256-batch-32",
                LlamaContextParams::default()
                    .with_n_ctx(NonZeroU32::new(256))
                    .with_n_batch(32)
                    .with_n_ubatch(32)
                    .with_op_offload(false)
                    .with_n_threads(thread_count)
                    .with_n_threads_batch(thread_count),
            ),
            (
                "ctx-512-batch-64",
                LlamaContextParams::default()
                    .with_n_ctx(NonZeroU32::new(512))
                    .with_n_batch(64)
                    .with_n_ubatch(64)
                    .with_op_offload(false)
                    .with_n_threads(thread_count)
                    .with_n_threads_batch(thread_count),
            ),
            (
                "ctx-1024-batch-128",
                LlamaContextParams::default()
                    .with_n_ctx(NonZeroU32::new(1024))
                    .with_n_batch(128)
                    .with_n_ubatch(128)
                    .with_op_offload(false)
                    .with_n_threads(thread_count)
                    .with_n_threads_batch(thread_count),
            ),
            (
                "ctx-2048-batch-256",
                LlamaContextParams::default()
                    .with_n_ctx(NonZeroU32::new(2048))
                    .with_n_batch(256)
                    .with_n_ubatch(256)
                    .with_op_offload(false)
                    .with_n_threads(thread_count)
                    .with_n_threads_batch(thread_count),
            ),
            (
                "ctx-4096-batch-256",
                LlamaContextParams::default()
                    .with_n_ctx(NonZeroU32::new(4096))
                    .with_n_batch(256)
                    .with_n_ubatch(256)
                    .with_op_offload(false)
                    .with_n_threads(thread_count)
                    .with_n_threads_batch(thread_count),
            ),
        ];

        let mut successful_attempts = Vec::new();
        let mut failed_attempts = Vec::new();

        probe_contexts(
            &backend,
            &model,
            "default-model",
            attempts,
            &mut successful_attempts,
            &mut failed_attempts,
        );

        if successful_attempts.is_empty() {
            if let Some(cpu_device_index) = cpu_device_index {
                eprintln!("retrying_with_explicit_cpu_device={cpu_device_index}");

                let explicit_cpu_model_params = LlamaModelParams::default()
                    .with_n_gpu_layers(0)
                    .with_split_mode(LlamaSplitMode::None)
                    .with_devices(&[cpu_device_index])
                    .expect("CPU backend device should be selectable");

                match LlamaModel::load_from_file(&backend, &model_path, &explicit_cpu_model_params)
                {
                    Ok(explicit_cpu_model) => {
                        eprintln!("model_load[explicit-cpu]=ok");
                        probe_contexts(
                            &backend,
                            &explicit_cpu_model,
                            "explicit-cpu-model",
                            vec![
                                (
                                    "default-cpu-only",
                                    LlamaContextParams::default().with_op_offload(false),
                                ),
                                (
                                    "ctx-256-batch-32",
                                    LlamaContextParams::default()
                                        .with_n_ctx(NonZeroU32::new(256))
                                        .with_n_batch(32)
                                        .with_n_ubatch(32)
                                        .with_op_offload(false)
                                        .with_n_threads(thread_count)
                                        .with_n_threads_batch(thread_count),
                                ),
                            ],
                            &mut successful_attempts,
                            &mut failed_attempts,
                        );
                    }
                    Err(err) => {
                        eprintln!("model_load[explicit-cpu]=err {err}");
                        failed_attempts.push(format!("explicit-cpu-model-load: {err}"));
                    }
                }
            } else {
                failed_attempts
                    .push("explicit-cpu-model-load: CPU backend device not found".into());
            }
        }

        if successful_attempts.is_empty() {
            eprintln!(
                "all context attempts failed, including explicit CPU device retry when available"
            );
        }

        assert!(
            !successful_attempts.is_empty(),
            "no llama context configuration succeeded for '{}'\n{}",
            model_path.display(),
            failed_attempts.join("\n")
        );
    }

    #[tokio::test]
    #[ignore = "requires MR_MILCHICK_LLM_MODEL_PATH and runs real GGUF inference"]
    async fn reacts_to_typescript_frontend_changes() {
        let snapshot = sample_snapshot(
            "Render raw profile bios from the API payload",
            "Updates the profile card to render API-provided HTML, bypasses the sanitizer helper, and removes coverage around safe rendering and loading states.",
            &["frontend", "typescript"],
            vec![
                changed_file(
                    "apps/frontend/src/components/ProfileBio.tsx",
                    r#"@@ -1,15 +1,27 @@
-import { useMemo } from "react";
+import { useEffect, useState } from "react";
+import { profileApi } from "../services/profileApi";
 
 export function ProfileBio({ userId }: { userId: string }) {
-  const bio = useMemo(() => "Loading...", []);
-  return <p className="prose">{bio}</p>;
+  const [bioHtml, setBioHtml] = useState<string | null>(null);
 
   useEffect(() => {
-    fetch(`/api/users/${userId}/bio`)
-      .then((response) => response.text())
-      .then(setBioHtml);
+    let cancelled = false;
+
+    fetch(`/api/users/${userId}/bio`)
+      .then((response) => response.text())
+      .then((html) => {
+        if (!cancelled) {
+          setBioHtml(html);
+          profileApi.trackBioView(userId);
+        }
+      });
+
+    return () => {
+      cancelled = true;
+    };
   }, [userId]);
 
-  return <div className="prose" dangerouslySetInnerHTML={{ __html: bioHtml }} />;
+  if (bioHtml === null) {
+    return <p className="prose">Loading bio...</p>;
+  }
+
+  return <div className="prose" dangerouslySetInnerHTML={{ __html: bioHtml }} />;
 }
"#,
                ),
                changed_file(
                    "apps/frontend/src/app/api/users/[userId]/bio/route.ts",
                    r#"@@ -1,13 +1,11 @@
 import { NextResponse } from "next/server";
 import { getUserProfile } from "@/data/getUserProfile";
-import { sanitizeBioHtml } from "@/security/sanitizeBioHtml";
 
 export async function GET(
   _request: Request,
   { params }: { params: { userId: string } }
 ) {
   const profile = await getUserProfile(params.userId);
-  const safeBio = sanitizeBioHtml(profile.bioHtml ?? "");
-  return NextResponse.json({ bioHtml: safeBio });
+  return new NextResponse(profile.bioHtml ?? "", {
+    headers: { "content-type": "text/html; charset=utf-8" },
+  });
 }
"#,
                ),
                deleted_file(
                    "apps/frontend/src/security/sanitizeBioHtml.ts",
                    r#"@@ -1,12 +0,0 @@
-import DOMPurify from "dompurify";
-
-export function sanitizeBioHtml(value: string): string {
-  return DOMPurify.sanitize(value, {
-    ALLOWED_TAGS: ["b", "i", "em", "strong", "a", "p", "ul", "li", "br"],
-    ALLOWED_ATTR: ["href", "target", "rel"],
-  });
-}
"#,
                ),
                deleted_file(
                    "apps/frontend/src/components/ProfileBio.test.tsx",
                    r#"@@ -1,28 +0,0 @@
-import { render, screen, waitFor } from "@testing-library/react";
-import { ProfileBio } from "./ProfileBio";
-import * as profileApi from "../services/profileApi";
-
-beforeEach(() => {
-  vi.spyOn(window, "fetch").mockResolvedValue({
-    text: () => Promise.resolve("<script>alert(1)</script><p>Hello</p>"),
-  } as Response);
-});
-
-it("renders sanitized HTML bios", async () => {
-  vi.spyOn(profileApi, "trackBioView").mockResolvedValue(undefined);
-
-  render(<ProfileBio userId="user-1" />);
-
-  expect(screen.getByText("Loading bio...")).toBeInTheDocument();
-
-  await waitFor(() => {
-    expect(screen.getByText("Hello")).toBeInTheDocument();
-  });
-
-  expect(screen.queryByText("alert(1)")).not.toBeInTheDocument();
-  expect(profileApi.trackBioView).toHaveBeenCalledWith("user-1");
-});
"#,
                ),
            ],
        );

        run_reaction_case(
            "typescript-frontend",
            snapshot,
            &[
                "sanitize",
                "xss",
                "dangerouslysetinnerhtml",
                "html",
                "test",
                "coverage",
                "loading",
            ],
            &[
                &["sanitize", "xss", "dangerouslysetinnerhtml", "html"],
                &["test", "coverage", "loading", "assert"],
            ],
            &[
                RecommendationCategory::Risk,
                RecommendationCategory::TestGap,
            ],
            2,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires MR_MILCHICK_LLM_MODEL_PATH and runs real GGUF inference"]
    async fn reacts_to_javascript_backend_changes() {
        let snapshot = sample_snapshot(
            "Simplify admin role updates",
            "Cleans up the admin role endpoint, inlines the role mutation path, and removes authorization and validation coverage that used to guard the route.",
            &["backend", "javascript"],
            vec![
                changed_file(
                    "apps/api/routes/admin.js",
                    r#"@@ -1,19 +1,16 @@
 const express = require("express");
-const requireAdmin = require("../auth/requireAdmin");
-const validateRoleChange = require("../validation/validateRoleChange");
+const { auditRoleChange } = require("../services/auditRoleChange");
+const { changeUserRole } = require("../services/changeUserRole");
 const router = express.Router();
 
-router.post("/users/:id/role", requireAdmin, validateRoleChange, updateRole);
+router.post("/users/:id/role", async (req, res, next) => {
+  try {
+    const updatedUser = await changeUserRole(req.params.id, req.body.role);
+    await auditRoleChange(req.user.id, updatedUser.id, req.body.role);
+    res.json({ ok: true, user: updatedUser });
+  } catch (error) {
+    next(error);
+  }
+});
 
 module.exports = router;
"#,
                ),
                changed_file(
                    "apps/api/services/changeUserRole.js",
                    r#"@@ -1,17 +1,11 @@
 const { userRepository } = require("../repositories/userRepository");
-const { ensureAllowedRole } = require("../validation/ensureAllowedRole");
-const { forbidProtectedUserMutation } = require("../auth/forbidProtectedUserMutation");
 
 async function changeUserRole(userId, role) {
-  ensureAllowedRole(role);
   const user = await userRepository.findById(userId);
-  forbidProtectedUserMutation(user);
-  return userRepository.updateRole(user.id, role);
+  return userRepository.updateRole(user.id, role);
 }
 
 module.exports = {
   changeUserRole,
 };
"#,
                ),
                deleted_file(
                    "apps/api/routes/admin.test.js",
                    r#"@@ -1,28 +0,0 @@
-it("rejects non-admin role updates", async () => {
-  const response = await request(app)
-    .post("/users/123/role")
-    .set("x-user-role", "viewer")
-    .send({ role: "staff" });
-
-  expect(response.status).toBe(403);
-});
-
-it("rejects invalid roles", async () => {
-  const response = await request(app)
-    .post("/users/123/role")
-    .set("x-user-role", "admin")
-    .send({ role: "godmode" });
-
-  expect(response.status).toBe(400);
-  expect(response.body.error).toContain("invalid role");
-});
"#,
                ),
            ],
        );

        run_reaction_case(
            "javascript-backend",
            snapshot,
            &[
                "auth",
                "authorization",
                "admin",
                "permission",
                "validation",
                "security",
                "role",
                "access",
                "test",
            ],
            &[
                &["auth", "authorization", "admin", "permission", "access"],
                &["validation", "invalid", "role"],
                &["test", "coverage", "assert"],
            ],
            &[
                RecommendationCategory::Risk,
                RecommendationCategory::TestGap,
            ],
            2,
        )
        .await;
    }
}
