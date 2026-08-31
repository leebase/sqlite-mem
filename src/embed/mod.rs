//! The embedder: granite-embedding-small-english-r2 under Candle, with a
//! deterministic `Fixed` fake for tests that must run with no network and
//! no model weights (`test-support` feature; pattern lifted from Satchel's
//! `src/embed/mod.rs`, MIT, virgilvox/satchel -- see THIRD-PARTY.md).
//!
//! Loader, CLS pooling and L2 normalization for the model are adapted from
//! the S1 spike (`spike/embed-parity/rust/src/main.rs`), which itself
//! adapted the same shape from Satchel. The memory-efficient ModernBERT
//! forward pass lives in `modernbert_mem` (see that module's docs).
//!
//! Two build paths for real Candle inference:
//!   * `model-sidecar` (dev default) -- loads weights/tokenizer/config from
//!     a directory on disk, named by `SQLITE_MEM_MODEL_DIR`. Fast
//!     incremental builds; nothing is linked into the binary.
//!   * `embed-model` (release builds only, not exercised by CI) -- weights
//!     linked in via `include_bytes!`.
//!
//! Candle is pinned to exactly 0.9.1 with tokenizers/fancy-regex
//! (architecture.md §7 changelog, S2 close): the S1 parity harness passed
//! against this exact configuration and the dependency tree is fully
//! pure-Rust (no oniguruma C dep), simplifying musl builds.

#[cfg(any(feature = "model-sidecar", feature = "embed-model"))]
mod modernbert_mem;

/// The embedder identity recorded in `db_info` at DB creation
/// (architecture.md §7, §19; decisions.md D014).
pub const EMBEDDER_ID: &str = "granite-embedding-small-english-r2";
pub const EMBEDDER_DIMS: usize = 384;

pub struct Embedder {
    inner: Inner,
}

enum Inner {
    #[cfg(any(feature = "model-sidecar", feature = "embed-model"))]
    Candle(Box<candle_backend::CandleEmbedder>),
    #[cfg(feature = "test-support")]
    Fixed,
}

impl Embedder {
    /// Loads the embedder per the priority described in the module docs.
    ///
    /// Every branch below is behind its own `#[cfg(...)]`; depending on
    /// which features are enabled, a different branch ends up as the
    /// function's last statement, so `clippy::needless_return` fires on
    /// some feature combinations but not others -- allowed at the function
    /// level rather than churning the `return`s per build.
    #[allow(clippy::needless_return)]
    pub fn load() -> Result<Self, crate::error::AppError> {
        #[cfg(feature = "test-support")]
        if force_fixed_via_env() {
            return Ok(Embedder {
                inner: Inner::Fixed,
            });
        }

        #[cfg(any(feature = "model-sidecar", feature = "embed-model"))]
        {
            return Ok(Embedder {
                inner: Inner::Candle(Box::new(candle_backend::CandleEmbedder::load()?)),
            });
        }

        #[cfg(all(
            feature = "test-support",
            not(any(feature = "model-sidecar", feature = "embed-model"))
        ))]
        {
            return Ok(Embedder {
                inner: Inner::Fixed,
            });
        }

        #[cfg(not(any(
            feature = "model-sidecar",
            feature = "embed-model",
            feature = "test-support"
        )))]
        {
            compile_error!(
                "sqlite-mem: enable at least one of the `model-sidecar`, `embed-model`, or `test-support` features"
            );
        }
    }

    /// Embeds `text` (already chunked to the product's bounds), returning
    /// an L2-normalized `dims()`-length vector.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>, crate::error::AppError> {
        match &self.inner {
            #[cfg(any(feature = "model-sidecar", feature = "embed-model"))]
            Inner::Candle(c) => c.embed(text),
            #[cfg(feature = "test-support")]
            Inner::Fixed => Ok(fixed_vector(text)),
        }
    }

    pub fn dims(&self) -> usize {
        EMBEDDER_DIMS
    }

    #[allow(dead_code)] // used by `ask`/`reindex` in later sprints; save.rs uses the crate-level constant directly
    pub fn id(&self) -> &'static str {
        EMBEDDER_ID
    }
}

/// A deterministic fake embedding: seeded from the text's own byte sum so
/// distinct texts get distinct (but still fixed/reproducible) vectors --
/// useful for save-path tests that check per-chunk embeddings landed in
/// distinct BLOB rows, while remaining fully offline (Satchel's `Fixed`
/// pattern, extended slightly: Satchel's fixed vector is content-invariant,
/// which we didn't need here).
#[cfg(feature = "test-support")]
fn fixed_vector(text: &str) -> Vec<f32> {
    let seed: u32 = text
        .bytes()
        .fold(1u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let mut v = vec![0f32; EMBEDDER_DIMS];
    let idx = (seed as usize) % EMBEDDER_DIMS;
    v[idx] = 1.0;
    v
}

#[cfg(feature = "test-support")]
fn force_fixed_via_env() -> bool {
    std::env::var("SQLITE_MEM_FIXED_EMBEDDER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[cfg(any(feature = "model-sidecar", feature = "embed-model"))]
mod candle_backend {
    use super::modernbert_mem::ModernBertMemLite;
    use crate::error::AppError;
    use candle_core::{DType, Device, IndexOp, Tensor};
    use candle_nn::VarBuilder;
    use candle_transformers::models::modernbert::Config as ModernBertConfig;
    use tokenizers::{Tokenizer, TruncationDirection, TruncationParams, TruncationStrategy};

    #[cfg(feature = "embed-model")]
    mod embedded {
        // Read-only reference to the S1 spike's already-downloaded model
        // files; embed-model is a release-only feature not exercised by
        // CI (project instructions: never modify spike/, and never
        // download models in this sprint).
        pub const MODEL: &[u8] =
            include_bytes!("../../spike/embed-parity/models/granite/model.f16.safetensors");
        pub const TOKENIZER: &[u8] =
            include_bytes!("../../spike/embed-parity/models/granite/tokenizer.json");
        pub const CONFIG: &[u8] =
            include_bytes!("../../spike/embed-parity/models/granite/config.json");
    }

    /// Raw fields needed out of ModernBERT's config.json. Built by hand
    /// rather than deserialized straight into candle's `Config` because
    /// candle flattens an optional classifier config into the same
    /// struct, and granite's config.json carries `classifier_pooling`
    /// without `id2label`/`label2id` (ported from the S1 spike).
    #[derive(serde::Deserialize)]
    struct ModernBertRawConfig {
        vocab_size: usize,
        hidden_size: usize,
        num_hidden_layers: usize,
        num_attention_heads: usize,
        intermediate_size: usize,
        max_position_embeddings: usize,
        layer_norm_eps: f64,
        pad_token_id: u32,
        global_attn_every_n_layers: usize,
        global_rope_theta: f64,
        local_attention: usize,
        local_rope_theta: f64,
    }

    fn parse_config(bytes: &[u8]) -> Result<ModernBertConfig, AppError> {
        let raw: ModernBertRawConfig = serde_json::from_slice(bytes).map_err(|e| {
            AppError::database(
                "embedder_load_failed",
                format!("parsing ModernBERT config.json: {e}"),
            )
        })?;
        Ok(ModernBertConfig {
            vocab_size: raw.vocab_size,
            hidden_size: raw.hidden_size,
            num_hidden_layers: raw.num_hidden_layers,
            num_attention_heads: raw.num_attention_heads,
            intermediate_size: raw.intermediate_size,
            max_position_embeddings: raw.max_position_embeddings,
            layer_norm_eps: raw.layer_norm_eps,
            pad_token_id: raw.pad_token_id,
            global_attn_every_n_layers: raw.global_attn_every_n_layers,
            global_rope_theta: raw.global_rope_theta,
            local_attention: raw.local_attention,
            local_rope_theta: raw.local_rope_theta,
            classifier_config: None,
        })
    }

    pub struct CandleEmbedder {
        model: ModernBertMemLite,
        tokenizer: Tokenizer,
        device: Device,
    }

    impl CandleEmbedder {
        #[allow(clippy::needless_return)] // same rationale as Embedder::load above
        pub fn load() -> Result<Self, AppError> {
            #[cfg(feature = "model-sidecar")]
            if let Ok(dir) = std::env::var("SQLITE_MEM_MODEL_DIR") {
                return Self::from_disk(&dir);
            }
            // Reached when: embed-model is enabled (dev override above did
            // not apply or model-sidecar is off), or model-sidecar is the
            // only backend and no SQLITE_MEM_MODEL_DIR was set.
            #[cfg(feature = "embed-model")]
            {
                return Self::from_embedded();
            }
            #[cfg(all(feature = "model-sidecar", not(feature = "embed-model")))]
            {
                return Err(AppError::database(
                    "embedder_load_failed",
                    "no embedding model available: set SQLITE_MEM_MODEL_DIR to a directory containing config.json, tokenizer.json, and model.safetensors",
                )
                .with_hint("SQLITE_MEM_MODEL_DIR=spike/embed-parity/models/granite (dev/test only)"));
            }
        }

        #[cfg(feature = "model-sidecar")]
        fn from_disk(dir: &str) -> Result<Self, AppError> {
            use std::path::Path;
            let dir = Path::new(dir);
            let weights_name = std::env::var("SQLITE_MEM_MODEL_WEIGHTS")
                .unwrap_or_else(|_| "model.safetensors".to_string());
            let model_path = dir.join(weights_name);
            let config_bytes = std::fs::read(dir.join("config.json")).map_err(|e| {
                AppError::database(
                    "embedder_load_failed",
                    format!("reading {}: {e}", dir.join("config.json").display()),
                )
            })?;
            let tokenizer_bytes = std::fs::read(dir.join("tokenizer.json")).map_err(|e| {
                AppError::database(
                    "embedder_load_failed",
                    format!("reading {}: {e}", dir.join("tokenizer.json").display()),
                )
            })?;
            let device = Device::Cpu;
            // SAFETY: the safetensors file is read-only and not modified
            // while mapped (same invariant as the S1 spike and Satchel).
            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(&[&model_path], DType::F32, &device).map_err(
                    |e| {
                        AppError::database(
                            "embedder_load_failed",
                            format!("mmapping {}: {e}", model_path.display()),
                        )
                    },
                )?
            };
            Self::build(vb, &config_bytes, &tokenizer_bytes, device)
        }

        #[cfg(feature = "embed-model")]
        fn from_embedded() -> Result<Self, AppError> {
            let device = Device::Cpu;
            let vb = VarBuilder::from_slice_safetensors(embedded::MODEL, DType::F32, &device)
                .map_err(|e| {
                    AppError::database(
                        "embedder_load_failed",
                        format!("loading embedded safetensors: {e}"),
                    )
                })?;
            Self::build(vb, embedded::CONFIG, embedded::TOKENIZER, device)
        }

        #[cfg(any(feature = "model-sidecar", feature = "embed-model"))]
        fn build(
            vb: VarBuilder,
            config_bytes: &[u8],
            tokenizer_bytes: &[u8],
            device: Device,
        ) -> Result<Self, AppError> {
            let cfg = parse_config(config_bytes)?;
            if cfg.hidden_size != super::EMBEDDER_DIMS {
                return Err(AppError::database(
                    "embedder_load_failed",
                    format!(
                        "model hidden_size {} does not match the product's declared dims {}",
                        cfg.hidden_size,
                        super::EMBEDDER_DIMS
                    ),
                ));
            }
            let model = ModernBertMemLite::load(vb, &cfg).map_err(|e| {
                AppError::database(
                    "embedder_load_failed",
                    format!("loading ModernBERT weights: {e}"),
                )
            })?;

            let mut tokenizer = Tokenizer::from_bytes(tokenizer_bytes).map_err(|e| {
                AppError::database("embedder_load_failed", format!("loading tokenizer: {e}"))
            })?;
            // HF-equivalent truncation: `tokenizers` subtracts the
            // post-processor's added special tokens from max_length, so
            // content is capped at max_seq_len - 2 and the sequence stays
            // [CLS] ... [SEP]. Truncation is set to the model's real
            // capacity (8192): sqlite-mem bounds chunks to ~1024 tokens
            // before this call, so truncation here is a safety net for the
            // char-based approximation in `chunk`, not the primary bound.
            tokenizer
                .with_truncation(Some(TruncationParams {
                    max_length: cfg.max_position_embeddings,
                    strategy: TruncationStrategy::LongestFirst,
                    direction: TruncationDirection::Right,
                    stride: 0,
                }))
                .map_err(|e| {
                    AppError::database(
                        "embedder_load_failed",
                        format!("configuring truncation: {e}"),
                    )
                })?;

            Ok(CandleEmbedder {
                model,
                tokenizer,
                device,
            })
        }

        pub fn embed(&self, text: &str) -> Result<Vec<f32>, AppError> {
            let encoding = self.tokenizer.encode(text, true).map_err(|e| {
                AppError::database("embedding_failed", format!("tokenization failed: {e}"))
            })?;
            let input_ids: Vec<u32> = encoding.get_ids().to_vec();

            let input_ids_t = Tensor::new(&input_ids[..], &self.device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(|e| AppError::database("embedding_failed", e.to_string()))?;

            let hidden = self
                .model
                .forward(&input_ids_t)
                .map_err(|e| AppError::database("embedding_failed", e.to_string()))?;

            // granite's `modules.json` has no normalize module (S1 finding
            // F5): pooling and L2 normalization are the product's job.
            let pooled = hidden
                .to_dtype(DType::F32)
                .and_then(|h| h.i((.., 0))) // CLS pooling, per 1_Pooling/config.json
                .map_err(|e| AppError::database("embedding_failed", e.to_string()))?;
            let norm = pooled
                .sqr()
                .and_then(|t| t.sum(1))
                .and_then(|t| t.sqrt())
                .map_err(|e| AppError::database("embedding_failed", e.to_string()))?;
            let normalized = pooled
                .broadcast_div(
                    &norm
                        .unsqueeze(1)
                        .map_err(|e| AppError::database("embedding_failed", e.to_string()))?,
                )
                .map_err(|e| AppError::database("embedding_failed", e.to_string()))?;
            let v: Vec<f32> = normalized
                .squeeze(0)
                .and_then(|t| t.to_vec1())
                .map_err(|e| AppError::database("embedding_failed", e.to_string()))?;
            Ok(v)
        }
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    use super::*;

    #[test]
    fn fixed_embedder_is_deterministic() {
        std::env::set_var("SQLITE_MEM_FIXED_EMBEDDER", "1");
        let e = Embedder::load().unwrap();
        let a = e.embed("hello world").unwrap();
        let b = e.embed("hello world").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), EMBEDDER_DIMS);
        std::env::remove_var("SQLITE_MEM_FIXED_EMBEDDER");
    }

    #[test]
    fn fixed_embedder_dims_match_product_constant() {
        std::env::set_var("SQLITE_MEM_FIXED_EMBEDDER", "1");
        let e = Embedder::load().unwrap();
        assert_eq!(e.dims(), 384);
        assert_eq!(e.id(), "granite-embedding-small-english-r2");
        std::env::remove_var("SQLITE_MEM_FIXED_EMBEDDER");
    }
}
