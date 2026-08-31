//! sqlite-mem Sprint S1 — embedding parity spike (Rust / Candle side).
//!
//! Loads an embedding model under Candle (pure Rust, CPU) and writes the
//! embedding of every text in a JSONL corpus to a JSON file, so a Python
//! sentence-transformers reference run can be compared against it.
//!
//! Two model families are supported:
//!   * `bge`     — BAAI/bge-small-en-v1.5, plain BERT, 512 ctx, CLS pooling.
//!   * `granite` — ibm-granite/granite-embedding-small-english-r2,
//!                 ModernBERT, 8192 ctx, CLS pooling (per 1_Pooling/config.json).
//!
//! The BERT path (loader, CLS-vs-mean pooling, truncation handling and L2
//! normalization) is adapted from Satchel `src/embed/mod.rs`
//! (MIT, virgilvox/satchel).

use anyhow::{anyhow, Context, Result};
use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use candle_transformers::models::modernbert::{Config as ModernBertConfig, ModernBert};

mod modernbert_mem;
use modernbert_mem::ModernBertMemLite;
use clap::Parser;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokenizers::{Tokenizer, TruncationDirection, TruncationParams, TruncationStrategy};

// ─────────────────────────────────────────────────────────────────────────────
// Embedded weights (cargo feature `embed-model`)
//
// Granite f16 safetensors + tokenizer + config linked into the binary via
// include_bytes!, so the process needs no files on disk at all. Produce the
// f16 file first with:  embed-parity convert-f16 ...
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(feature = "embed-model")]
mod embedded {
    pub const MODEL: &[u8] = include_bytes!("../../models/granite/model.f16.safetensors");
    pub const TOKENIZER: &[u8] = include_bytes!("../../models/granite/tokenizer.json");
    pub const CONFIG: &[u8] = include_bytes!("../../models/granite/config.json");
}

#[derive(Parser, Debug)]
#[command(name = "embed-parity", about = "Candle-side embedding parity spike")]
struct Args {
    /// Which model to run: granite | bge
    #[arg(long, default_value = "granite")]
    model: String,

    /// Directory holding <model>/{model.safetensors,config.json,tokenizer.json}
    #[arg(long)]
    models_dir: Option<PathBuf>,

    /// Corpus JSONL: one {"id","text"} object per line
    #[arg(long)]
    corpus: Option<PathBuf>,

    /// Output JSON path
    #[arg(long)]
    out: Option<PathBuf>,

    /// Embed a single ad-hoc text instead of a corpus (prints JSON to stdout)
    #[arg(long)]
    text: Option<String>,

    /// Load weights/tokenizer/config from the binary itself (needs feature `embed-model`)
    #[arg(long)]
    embedded: bool,

    /// Override the safetensors file (e.g. an f16 conversion)
    #[arg(long)]
    weights: Option<PathBuf>,

    /// Compute dtype: f32 (parity default) or f16
    #[arg(long, default_value = "f32")]
    dtype: String,

    /// Restrict a corpus run to a single id (used for the f16-vs-f32 sanity check)
    #[arg(long)]
    only_id: Option<String>,

    /// Convert the model's safetensors to f16 and exit (writes to --out)
    #[arg(long)]
    convert_f16: bool,

    /// ModernBERT forward implementation: memlite (default, memory-efficient)
    /// or stock (candle-transformers' own module — OOMs above ~4k tokens).
    #[arg(long = "impl", default_value = "memlite")]
    impl_: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Model spec
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Family {
    Bert,
    ModernBert,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pooling {
    /// Hidden state of token 0 ([CLS]). Both S1 models use this
    /// (1_Pooling/config.json: pooling_mode_cls_token = true).
    Cls,
    #[allow(dead_code)]
    Mean,
}

struct Spec {
    name: &'static str,
    hf_id: &'static str,
    family: Family,
    /// sentence_bert_config.json: max_seq_length (includes [CLS]/[SEP])
    max_seq_len: usize,
    pooling: Pooling,
}

fn spec_for(model: &str) -> Result<Spec> {
    match model {
        "granite" => Ok(Spec {
            name: "granite",
            hf_id: "ibm-granite/granite-embedding-small-english-r2",
            family: Family::ModernBert,
            max_seq_len: 8192,
            pooling: Pooling::Cls,
        }),
        "bge" => Ok(Spec {
            name: "bge",
            hf_id: "BAAI/bge-small-en-v1.5",
            family: Family::Bert,
            max_seq_len: 512,
            pooling: Pooling::Cls,
        }),
        other => Err(anyhow!("unknown model '{other}' (expected granite|bge)")),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Embedder
// ─────────────────────────────────────────────────────────────────────────────

enum Backbone {
    Bert(BertModel),
    /// candle-transformers' stock module (kept for cross-checking).
    ModernBertStock(ModernBert),
    /// Memory-efficient equivalent, see `modernbert_mem`.
    ModernBertMemLite(ModernBertMemLite),
}

struct Embedder {
    backbone: Backbone,
    tokenizer: Tokenizer,
    device: Device,
    dtype: DType,
    dims: usize,
    pooling: Pooling,
}

/// Granite ships a bare `ModernBertModel` checkpoint: tensors are named
/// `embeddings.*` / `layers.N.*` / `final_norm.*`. candle-transformers'
/// `ModernBert::load` addresses them under a `model.` prefix (it was written
/// against `ModernBertForMaskedLM` checkpoints), so strip that prefix on lookup.
fn strip_model_prefix(name: &str) -> String {
    name.strip_prefix("model.").unwrap_or(name).to_string()
}

/// Raw fields we need out of a ModernBERT config.json. Built by hand rather
/// than deserialized straight into candle's `Config` because candle flattens an
/// optional classifier config into the same struct, and granite's config.json
/// carries `classifier_pooling` without `id2label`/`label2id`.
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

fn modernbert_config(bytes: &[u8]) -> Result<ModernBertConfig> {
    let raw: ModernBertRawConfig =
        serde_json::from_slice(bytes).context("parsing ModernBERT config.json")?;
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

/// Try `BertModel::load` at the root, then under a `bert.` submodule.
/// (Satchel pattern — MIT, virgilvox/satchel.)
fn build_bert(vb: VarBuilder, config: &BertConfig) -> Result<BertModel> {
    match BertModel::load(vb.clone(), config) {
        Ok(m) => Ok(m),
        Err(first) => BertModel::load(vb.pp("bert"), config).map_err(|_| {
            anyhow!("failed to load BERT weights (root and bert.* both rejected): {first}")
        }),
    }
}

impl Embedder {
    fn from_disk(
        spec: &Spec,
        models_dir: &Path,
        weights: Option<&Path>,
        dtype: DType,
        impl_: &str,
    ) -> Result<Self> {
        let dir = models_dir.join(spec.name);
        let model_path = weights
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| dir.join("model.safetensors"));
        let config_bytes = std::fs::read(dir.join("config.json"))
            .with_context(|| format!("reading {}", dir.join("config.json").display()))?;
        let tokenizer_bytes = std::fs::read(dir.join("tokenizer.json"))
            .with_context(|| format!("reading {}", dir.join("tokenizer.json").display()))?;

        let device = Device::Cpu;
        // SAFETY: the safetensors file is read-only and not modified while mapped.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[&model_path], dtype, &device)
                .with_context(|| format!("mmaping {}", model_path.display()))?
        };
        Self::build(spec, vb, &config_bytes, &tokenizer_bytes, device, dtype, impl_)
    }

    #[cfg(feature = "embed-model")]
    fn from_embedded(spec: &Spec, dtype: DType, impl_: &str) -> Result<Self> {
        let device = Device::Cpu;
        let vb = VarBuilder::from_slice_safetensors(embedded::MODEL, dtype, &device)
            .context("loading embedded safetensors")?;
        Self::build(
            spec,
            vb,
            embedded::CONFIG,
            embedded::TOKENIZER,
            device,
            dtype,
            impl_,
        )
    }

    fn build(
        spec: &Spec,
        vb: VarBuilder,
        config_bytes: &[u8],
        tokenizer_bytes: &[u8],
        device: Device,
        dtype: DType,
        impl_: &str,
    ) -> Result<Self> {
        let (backbone, dims) = match spec.family {
            Family::Bert => {
                let cfg: BertConfig =
                    serde_json::from_slice(config_bytes).context("parsing BERT config.json")?;
                let dims = cfg.hidden_size;
                (Backbone::Bert(build_bert(vb, &cfg)?), dims)
            }
            Family::ModernBert => {
                let cfg = modernbert_config(config_bytes)?;
                let dims = cfg.hidden_size;
                let backbone = match impl_ {
                    "memlite" => Backbone::ModernBertMemLite(
                        ModernBertMemLite::load(vb, &cfg)
                            .context("loading ModernBERT weights (memlite)")?,
                    ),
                    "stock" => {
                        // candle addresses these tensors under a `model.` prefix.
                        let vb = vb.rename_f(strip_model_prefix);
                        Backbone::ModernBertStock(
                            ModernBert::load(vb, &cfg)
                                .context("loading ModernBERT weights (stock)")?,
                        )
                    }
                    other => return Err(anyhow!("unknown --impl '{other}' (memlite|stock)")),
                };
                (backbone, dims)
            }
        };

        let mut tokenizer = Tokenizer::from_bytes(tokenizer_bytes)
            .map_err(|e| anyhow!("loading tokenizer: {e}"))?;
        // HF-equivalent truncation: `tokenizers` subtracts the post-processor's
        // added special tokens from max_length, so content is capped at
        // max_seq_len - 2 and the sequence stays [CLS] ... [SEP].
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: spec.max_seq_len,
                strategy: TruncationStrategy::LongestFirst,
                direction: TruncationDirection::Right,
                stride: 0,
            }))
            .map_err(|e| anyhow!("configuring truncation: {e}"))?;

        Ok(Self {
            backbone,
            tokenizer,
            device,
            dtype,
            dims,
            pooling: spec.pooling,
        })
    }

    fn embed(&self, text: &str) -> Result<(Vec<f32>, usize)> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow!("tokenization failed: {e}"))?;
        let input_ids: Vec<u32> = encoding.get_ids().to_vec();
        let attention_mask: Vec<u32> = encoding.get_attention_mask().to_vec();
        let seq_len = input_ids.len();

        let input_ids_t = Tensor::new(&input_ids[..], &self.device)?.unsqueeze(0)?;
        let attention_mask_t = Tensor::new(&attention_mask[..], &self.device)?.unsqueeze(0)?;

        let hidden = match &self.backbone {
            Backbone::Bert(m) => {
                let token_type_ids: Vec<u32> = encoding.get_type_ids().to_vec();
                let tt = Tensor::new(&token_type_ids[..], &self.device)?.unsqueeze(0)?;
                m.forward(&input_ids_t, &tt, Some(&attention_mask_t))?
            }
            Backbone::ModernBertStock(m) => m.forward(&input_ids_t, &attention_mask_t)?,
            Backbone::ModernBertMemLite(m) => m.forward(&input_ids_t)?,
        };

        // Pool in f32 regardless of the weight dtype so an f16 run differs from
        // the f32 run only in the weights, not in the reduction arithmetic.
        let hidden = hidden.to_dtype(DType::F32)?;
        let pooled = match self.pooling {
            Pooling::Cls => hidden.i((.., 0))?,
            Pooling::Mean => {
                let m = attention_mask_t.to_dtype(DType::F32)?.unsqueeze(2)?;
                let summed = hidden.broadcast_mul(&m)?.sum(1)?;
                let denom = m.sum(1)?;
                summed.broadcast_div(&denom)?
            }
        };

        // L2 normalize.
        let norm = pooled.sqr()?.sum(1)?.sqrt()?;
        let normalized = pooled.broadcast_div(&norm.unsqueeze(1)?)?;
        let v: Vec<f32> = normalized.squeeze(0)?.to_vec1()?;
        Ok((v, seq_len))
    }

    fn dtype_label(&self) -> &'static str {
        match self.dtype {
            DType::F16 => "f16",
            DType::BF16 => "bf16",
            _ => "f32",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Output schema
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct Timings {
    model_load: f64,
    total_embed: f64,
    per_text_mean: f64,
}

#[derive(Serialize)]
struct Output {
    model: String,
    #[serde(rename = "impl")]
    impl_: String,
    dims: usize,
    weights_dtype: String,
    max_seq_len: usize,
    pooling: String,
    timings_ms: Timings,
    token_counts: BTreeMap<String, usize>,
    vectors: BTreeMap<String, Vec<f32>>,
}

// ─────────────────────────────────────────────────────────────────────────────

fn parse_dtype(s: &str) -> Result<DType> {
    match s {
        "f32" => Ok(DType::F32),
        "f16" => Ok(DType::F16),
        "bf16" => Ok(DType::BF16),
        other => Err(anyhow!("unsupported dtype '{other}'")),
    }
}

fn convert_to_f16(src: &Path, dst: &Path) -> Result<()> {
    let device = Device::Cpu;
    let tensors = candle_core::safetensors::load(src, &device)
        .with_context(|| format!("reading {}", src.display()))?;
    let mut out: std::collections::HashMap<String, Tensor> = std::collections::HashMap::new();
    for (k, v) in tensors {
        // Integer bookkeeping tensors (e.g. BERT's `embeddings.position_ids`)
        // must not be cast to a float dtype.
        let v = match v.dtype() {
            DType::F64 | DType::F32 | DType::BF16 | DType::F16 => v.to_dtype(DType::F16)?,
            _ => v,
        };
        out.insert(k, v);
    }
    candle_core::safetensors::save(&out, dst)
        .with_context(|| format!("writing {}", dst.display()))?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let spec = spec_for(&args.model)?;
    let dtype = parse_dtype(&args.dtype)?;

    if args.convert_f16 {
        let models_dir = args
            .models_dir
            .clone()
            .ok_or_else(|| anyhow!("--convert-f16 needs --models-dir"))?;
        let src = args
            .weights
            .clone()
            .unwrap_or_else(|| models_dir.join(spec.name).join("model.safetensors"));
        let dst = args
            .out
            .clone()
            .ok_or_else(|| anyhow!("--convert-f16 needs --out"))?;
        convert_to_f16(&src, &dst)?;
        eprintln!(
            "converted {} -> {} ({} bytes)",
            src.display(),
            dst.display(),
            std::fs::metadata(&dst)?.len()
        );
        return Ok(());
    }

    let load_start = Instant::now();
    let embedder = if args.embedded {
        #[cfg(feature = "embed-model")]
        {
            Embedder::from_embedded(&spec, dtype, &args.impl_)?
        }
        #[cfg(not(feature = "embed-model"))]
        {
            return Err(anyhow!(
                "--embedded requires building with --features embed-model"
            ));
        }
    } else {
        let models_dir = args
            .models_dir
            .clone()
            .ok_or_else(|| anyhow!("--models-dir is required unless --embedded"))?;
        Embedder::from_disk(&spec, &models_dir, args.weights.as_deref(), dtype, &args.impl_)?
    };
    let model_load_ms = load_start.elapsed().as_secs_f64() * 1000.0;

    // Single ad-hoc text mode (cold-start measurement).
    if let Some(text) = args.text.as_deref() {
        let t0 = Instant::now();
        let (v, tokens) = embedder.embed(text)?;
        let embed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let mut vectors = BTreeMap::new();
        vectors.insert("text".to_string(), v);
        let mut token_counts = BTreeMap::new();
        token_counts.insert("text".to_string(), tokens);
        let out = Output {
            model: spec.hf_id.to_string(),
            impl_: "candle".to_string(),
            dims: embedder.dims,
            weights_dtype: embedder.dtype_label().to_string(),
            max_seq_len: spec.max_seq_len,
            pooling: "cls".to_string(),
            timings_ms: Timings {
                model_load: model_load_ms,
                total_embed: embed_ms,
                per_text_mean: embed_ms,
            },
            token_counts,
            vectors,
        };
        let json = serde_json::to_string(&out)?;
        match args.out.as_deref() {
            Some(p) => std::fs::write(p, json)?,
            None => println!("{json}"),
        }
        return Ok(());
    }

    let corpus_path = args
        .corpus
        .clone()
        .ok_or_else(|| anyhow!("--corpus is required unless --text is given"))?;
    let corpus = std::fs::read_to_string(&corpus_path)
        .with_context(|| format!("reading {}", corpus_path.display()))?;

    let mut vectors: BTreeMap<String, Vec<f32>> = BTreeMap::new();
    let mut token_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_ms = 0.0f64;
    let mut n = 0usize;

    for (lineno, line) in corpus.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let rec: serde_json::Value = serde_json::from_str(line)
            .with_context(|| format!("{}:{}", corpus_path.display(), lineno + 1))?;
        let id = rec["id"]
            .as_str()
            .ok_or_else(|| anyhow!("line {} has no string id", lineno + 1))?
            .to_string();
        if let Some(only) = args.only_id.as_deref() {
            if id != only {
                continue;
            }
        }
        let text = rec["text"]
            .as_str()
            .ok_or_else(|| anyhow!("line {} has no string text", lineno + 1))?;

        let t0 = Instant::now();
        let (v, tokens) = embedder
            .embed(text)
            .with_context(|| format!("embedding {id}"))?;
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        total_ms += ms;
        n += 1;
        eprintln!("{id}\ttokens={tokens}\t{ms:.1}ms");
        vectors.insert(id.clone(), v);
        token_counts.insert(id, tokens);
    }

    let out = Output {
        model: spec.hf_id.to_string(),
        impl_: "candle".to_string(),
        dims: embedder.dims,
        weights_dtype: embedder.dtype_label().to_string(),
        max_seq_len: spec.max_seq_len,
        pooling: "cls".to_string(),
        timings_ms: Timings {
            model_load: model_load_ms,
            total_embed: total_ms,
            per_text_mean: if n > 0 { total_ms / n as f64 } else { 0.0 },
        },
        token_counts,
        vectors,
    };

    let json = serde_json::to_string(&out)?;
    match args.out.as_deref() {
        Some(p) => {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(p, json)?;
            eprintln!("wrote {} ({} texts)", p.display(), n);
        }
        None => println!("{json}"),
    }
    Ok(())
}
