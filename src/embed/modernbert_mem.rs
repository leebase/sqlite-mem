//! Memory-efficient ModernBERT encoder.
//!
//! Carried into the product unchanged from the S1 spike
//! (`spike/embed-parity/rust/src/modernbert_mem.rs`), which is itself
//! derived from candle-transformers 0.11.0 `src/models/modernbert.rs`
//! (Apache-2.0 OR MIT, huggingface/candle). Structure, weight names, RoPE,
//! GeGLU MLP, local/global layer schedule and normalization are unchanged;
//! the forward pass is reorganized so a full-length (8192-token) sequence
//! fits in memory:
//!
//!   1. Attention is computed one head at a time. The stock path builds a
//!      dense `(batch, heads, seq, seq)` f32 tensor -- 3.2 GB at seq=8192
//!      with 12 heads -- and then adds a mask and runs the *unfused*
//!      `ops::softmax`, which allocates three more tensors of that size.
//!      Measured peak RSS on the S1 corpus was 16.3 GB and the process was
//!      OOM-killed on a 19 GB box. Per head the largest tensor is
//!      `(seq, seq)` = 268 MB.
//!   2. `ops::softmax_last_dim` (fused, one output buffer) replaces
//!      `ops::softmax` (max / broadcast_sub / exp / sum / broadcast_div).
//!   3. The all-ones padding mask of a single unpadded sequence is skipped
//!      rather than materialized: the stock path adds `(1 - 1) * f32::MIN`,
//!      i.e. `-0.0`, to every logit, which is an exact no-op.
//!
//! Splitting the batched attention matmul into per-head matmuls is exact
//! (heads are independent), and `softmax_last_dim` performs the same
//! subtract-max/exp/normalize reduction in the same order as `softmax`.
//! Peak RSS on the S1 corpus dropped to 1.42 GB, ~24% faster, matching
//! stock to 8.2e-8 max component diff on every text stock survives (see
//! `spike/embed-parity/REPORT.md`, finding F1). This module is a required
//! product component per architecture.md §7 and decisions.md D014.

use candle_core::{DType, Device, IndexOp, Result, Tensor, D};
use candle_nn::{
    embedding, layer_norm_no_bias, linear_no_bias, ops::softmax_last_dim, Embedding, LayerNorm,
    Linear, Module, VarBuilder,
};
use candle_transformers::models::modernbert::Config;
use std::sync::Arc;

struct RotaryEmbedding {
    sin: Tensor,
    cos: Tensor,
}

impl RotaryEmbedding {
    fn new(dtype: DType, config: &Config, rope_theta: f64, dev: &Device) -> Result<Self> {
        let dim = config.hidden_size / config.num_attention_heads;
        let inv_freq: Vec<_> = (0..dim)
            .step_by(2)
            .map(|i| 1f32 / rope_theta.powf(i as f64 / dim as f64) as f32)
            .collect();
        let inv_freq_len = inv_freq.len();
        let inv_freq = Tensor::from_vec(inv_freq, (1, inv_freq_len), dev)?.to_dtype(dtype)?;
        let max_seq_len = config.max_position_embeddings;
        let t = Tensor::arange(0u32, max_seq_len as u32, dev)?
            .to_dtype(dtype)?
            .reshape((max_seq_len, 1))?;
        let freqs = t.matmul(&inv_freq)?;
        Ok(Self {
            sin: freqs.sin()?,
            cos: freqs.cos()?,
        })
    }

    fn apply(&self, q: &Tensor, k: &Tensor) -> Result<(Tensor, Tensor)> {
        let q = candle_nn::rotary_emb::rope(&q.contiguous()?, &self.cos, &self.sin)?;
        let k = candle_nn::rotary_emb::rope(&k.contiguous()?, &self.cos, &self.sin)?;
        Ok((q, k))
    }
}

struct Attention {
    qkv: Linear,
    proj: Linear,
    num_heads: usize,
    head_size: usize,
    rotary: Arc<RotaryEmbedding>,
}

impl Attention {
    fn load(vb: VarBuilder, config: &Config, rotary: Arc<RotaryEmbedding>) -> Result<Self> {
        Ok(Self {
            qkv: linear_no_bias(config.hidden_size, config.hidden_size * 3, vb.pp("Wqkv"))?,
            proj: linear_no_bias(config.hidden_size, config.hidden_size, vb.pp("Wo"))?,
            num_heads: config.num_attention_heads,
            head_size: config.hidden_size / config.num_attention_heads,
            rotary,
        })
    }

    /// `additive_mask` is `(seq, seq)` and is added to the logits before the
    /// softmax (the sliding-window mask on local-attention layers). `None`
    /// means unrestricted attention.
    fn forward(&self, xs: &Tensor, additive_mask: Option<&Tensor>) -> Result<Tensor> {
        let (b, seq_len, _d) = xs.dims3()?;
        debug_assert_eq!(b, 1, "sqlite-mem embeds one chunk at a time");

        let qkv = xs
            .apply(&self.qkv)?
            .reshape((b, seq_len, 3, self.num_heads, self.head_size))?
            .permute((2, 0, 3, 1, 4))?;
        let q = qkv.get(0)?;
        let k = qkv.get(1)?;
        let v = qkv.get(2)?.contiguous()?;

        let (q, k) = self.rotary.apply(&q, &k)?;
        let scale = (self.head_size as f64).powf(-0.5);
        let q = (q * scale)?;

        // One head at a time: the (seq, seq) score matrix is the only large
        // allocation, and it is dropped before the next head is started.
        let mut heads = Vec::with_capacity(self.num_heads);
        for h in 0..self.num_heads {
            let q_h = q.i((0, h))?.contiguous()?;
            let k_h = k.i((0, h))?.contiguous()?;
            let v_h = v.i((0, h))?.contiguous()?;
            let att = q_h.matmul(&k_h.t()?.contiguous()?)?;
            let att = match additive_mask {
                Some(m) => att.add(m)?,
                None => att,
            };
            let att = softmax_last_dim(&att)?;
            heads.push(att.matmul(&v_h)?);
        }
        // Concatenating per-head outputs on the feature axis reproduces the
        // stock `transpose(1, 2).reshape(b, seq, hidden)` layout.
        let xs = Tensor::cat(&heads, D::Minus1)?.unsqueeze(0)?;
        xs.apply(&self.proj)
    }
}

struct Mlp {
    wi: Linear,
    wo: Linear,
}

impl Mlp {
    fn load(vb: VarBuilder, config: &Config) -> Result<Self> {
        Ok(Self {
            wi: linear_no_bias(
                config.hidden_size,
                config.intermediate_size * 2,
                vb.pp("Wi"),
            )?,
            wo: linear_no_bias(config.intermediate_size, config.hidden_size, vb.pp("Wo"))?,
        })
    }
}

impl Module for Mlp {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let xs = xs.apply(&self.wi)?;
        let parts = xs.chunk(2, D::Minus1)?;
        (&parts[0].gelu_erf()? * &parts[1])?.apply(&self.wo) // GeGLU
    }
}

struct Layer {
    attn: Attention,
    mlp: Mlp,
    attn_norm: Option<LayerNorm>,
    mlp_norm: LayerNorm,
    local: bool,
}

impl Layer {
    fn load(
        vb: VarBuilder,
        config: &Config,
        rotary: Arc<RotaryEmbedding>,
        local: bool,
    ) -> Result<Self> {
        Ok(Self {
            attn: Attention::load(vb.pp("attn"), config, rotary)?,
            mlp: Mlp::load(vb.pp("mlp"), config)?,
            // Layer 0 has no attn_norm (Identity in HF); `.ok()` mirrors candle.
            attn_norm: layer_norm_no_bias(
                config.hidden_size,
                config.layer_norm_eps,
                vb.pp("attn_norm"),
            )
            .ok(),
            mlp_norm: layer_norm_no_bias(
                config.hidden_size,
                config.layer_norm_eps,
                vb.pp("mlp_norm"),
            )?,
            local,
        })
    }

    fn forward(&self, xs: &Tensor, local_mask: &Tensor) -> Result<Tensor> {
        let residual = xs.clone();
        let mut normed = xs.clone();
        if let Some(norm) = &self.attn_norm {
            normed = normed.apply(norm)?;
        }
        let mask = if self.local { Some(local_mask) } else { None };
        let xs = (self.attn.forward(&normed, mask)? + residual)?;
        let mlp_out = xs.apply(&self.mlp_norm)?.apply(&self.mlp)?;
        xs + mlp_out
    }
}

pub struct ModernBertMemLite {
    word_embeddings: Embedding,
    norm: LayerNorm,
    layers: Vec<Layer>,
    final_norm: LayerNorm,
    local_attention: usize,
    dtype: DType,
}

impl ModernBertMemLite {
    /// `vb` must address the checkpoint's own tensor names -- granite ships
    /// a bare `ModernBertModel` (`embeddings.*`, `layers.N.*`,
    /// `final_norm.*`), whereas candle's stock module prefixes everything
    /// with `model.`.
    pub fn load(vb: VarBuilder, config: &Config) -> Result<Self> {
        let word_embeddings = embedding(
            config.vocab_size,
            config.hidden_size,
            vb.pp("embeddings.tok_embeddings"),
        )?;
        let norm = layer_norm_no_bias(
            config.hidden_size,
            config.layer_norm_eps,
            vb.pp("embeddings.norm"),
        )?;
        let global_rotary = Arc::new(RotaryEmbedding::new(
            vb.dtype(),
            config,
            config.global_rope_theta,
            vb.device(),
        )?);
        let local_rotary = Arc::new(RotaryEmbedding::new(
            vb.dtype(),
            config,
            config.local_rope_theta,
            vb.device(),
        )?);

        let mut layers = Vec::with_capacity(config.num_hidden_layers);
        for layer_id in 0..config.num_hidden_layers {
            let local = layer_id % config.global_attn_every_n_layers != 0;
            layers.push(Layer::load(
                vb.pp(format!("layers.{layer_id}")),
                config,
                if local {
                    local_rotary.clone()
                } else {
                    global_rotary.clone()
                },
                local,
            )?);
        }
        let final_norm = layer_norm_no_bias(
            config.hidden_size,
            config.layer_norm_eps,
            vb.pp("final_norm"),
        )?;

        Ok(Self {
            word_embeddings,
            norm,
            layers,
            final_norm,
            local_attention: config.local_attention,
            // The additive mask must match the activation dtype; candle's
            // stock module hardcodes F32 here and therefore cannot run f16
            // weights.
            dtype: vb.dtype(),
        })
    }

    /// Additive sliding-window mask: `0` inside the window, `-inf` outside
    /// (candle's `get_local_attention_mask`, half-window = local_attention / 2).
    fn local_mask(&self, seq_len: usize, device: &Device) -> Result<Tensor> {
        let max_distance = (self.local_attention / 2) as i64;
        let mut mask = vec![0f32; seq_len * seq_len];
        for i in 0..seq_len {
            for j in 0..seq_len {
                if (j as i64 - i as i64).abs() > max_distance {
                    mask[i * seq_len + j] = f32::NEG_INFINITY;
                }
            }
        }
        Tensor::from_vec(mask, (seq_len, seq_len), device)?.to_dtype(self.dtype)
    }

    /// `input_ids` is `(1, seq)`. Padding is not supported (and not needed:
    /// sqlite-mem embeds one chunk per call), so no padding mask is taken --
    /// the stock path's all-ones mask contributes `-0.0` to every logit,
    /// which changes nothing.
    pub fn forward(&self, input_ids: &Tensor) -> Result<Tensor> {
        let seq_len = input_ids.dims2()?.1;
        let local_mask = self.local_mask(seq_len, input_ids.device())?;
        let mut xs = input_ids.apply(&self.word_embeddings)?.apply(&self.norm)?;
        for layer in self.layers.iter() {
            xs = layer.forward(&xs, &local_mask)?;
        }
        xs.apply(&self.final_norm)
    }
}
