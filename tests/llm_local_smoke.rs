#[cfg(feature = "llm-local")]
mod llm_local_smoke {
    use std::env;
    use std::num::NonZeroU32;
    use std::path::PathBuf;

    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::list_llama_ggml_backend_devices;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::model::LlamaModel;
    use llama_cpp_2::model::params::{LlamaModelParams, LlamaSplitMode};

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
        let model_path = PathBuf::from(
            env::var("MR_MILCHICK_LLM_MODEL_PATH")
                .expect("set MR_MILCHICK_LLM_MODEL_PATH to a local GGUF file"),
        );
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
}
