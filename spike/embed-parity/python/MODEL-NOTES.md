# Model notes — Sprint S1 embedding parity spike

Source of truth for these notes: each model's Hugging Face repo files (`config.json`,
`1_Pooling/config.json`, `sentence_bert_config.json`, `tokenizer_config.json`) and model
card, fetched 2026-08-31. `reference.py` deliberately encodes RAW text with **no**
query/document prefixes and **no** `prompt_name`, and leaves each model's own default
`max_seq_length` truncation untouched, to get an apples-to-apples parity baseline against
the Candle/Rust side. The prefix/instruction guidance below is recorded for later product
use, not applied in this spike.

## granite — `ibm-granite/granite-embedding-small-english-r2`

- **Pooling mode (from `1_Pooling/config.json`):** CLS-token pooling only —
  `pooling_mode_cls_token: true`, mean/max/mean_sqrt_len all `false`.
  word_embedding_dimension = 384.
  - Note: the base HF `config.json` for the underlying encoder sets
    `"classifier_pooling": "mean"`, but that field belongs to the raw
    `ModernBertModel` classifier head, not the sentence-transformers embedding
    pipeline. The sentence-transformers wrapper's `1_Pooling/config.json` (the module
    actually used by `SentenceTransformer.encode()`) overrides this with CLS pooling —
    that's the pooling actually exercised by `reference.py`.
- **max_seq_length:** 8192 tokens (`sentence_bert_config.json`: `"max_seq_length": 8192`;
  matches `max_position_embeddings: 8192` in the base config). `do_lower_case: false`.
  Longer texts are truncated to this length by sentence-transformers' default behavior
  (untouched in this spike).
- **Query/document prefixes for retrieval:** The model card does not define or require
  special query/passage prefix strings (no `"query: "` / `"passage: "` style instruction
  scheme). Usage examples encode queries and passages directly, unprefixed. Nothing to
  add for product use beyond what parity testing already does.
- **Tokenizer:** Architecture is `ModernBertModel` (`model_type: "modernbert"`,
  vocab_size 50368). ModernBERT uses a **BPE tokenizer** (a modified version of the
  OLMo/GPT-NeoX tokenizer, trained partly on code), *not* classic BERT WordPiece —
  despite reusing BERT-style special-token names (`[CLS]`/`[SEP]`) for backwards
  compatibility. `bos_token_id`/`cls_token_id` = 50281, `eos_token_id`/`sep_token_id`
  = 50282.
- **License:** Apache 2.0.

## bge — `BAAI/bge-small-en-v1.5`

- **Pooling mode (from `1_Pooling/config.json`):** CLS-token pooling only —
  `pooling_mode_cls_token: true`, mean/max/mean_sqrt_len all `false`.
  word_embedding_dimension = 384.
- **max_seq_length:** 512 tokens (`sentence_bert_config.json`: `"max_seq_length": 512`).
  `do_lower_case: true`. Any corpus text beyond ~512 tokens (several fixtures exceed
  this by a wide margin — the corpus intentionally includes texts >4096 characters/tokens)
  gets truncated by sentence-transformers' default tokenizer truncation before pooling.
  This is a materially more aggressive truncation than granite's 8192-token window, and
  is expected to be a source of divergence on the long fixtures — flagged here, not
  worked around, per the parity-spike instructions.
- **Query/document prefixes for retrieval:** BGE v1.5's model card recommends adding the
  instruction `"Represent this sentence for searching relevant passages: "` to **queries
  only** (not to the corpus/passage side) for retrieval use cases. The card notes that
  for v1.5 specifically, skipping the instruction causes only a "slight degradation" in
  retrieval quality, so it is optional/recommended rather than mandatory. Deliberately
  **not** applied in this spike (RAW text only, no prefixes) — record for later product
  use if/when bge is wired into actual query-time retrieval.
- **Tokenizer:** Standard BERT tokenizer — WordPiece, backed by `vocab.txt`
  (`AutoTokenizer`-compatible, uncased).
- **License:** MIT — "FlagEmbedding is licensed under the MIT License. The released
  models can be used for commercial purposes free of charge."

## Sources

- https://huggingface.co/ibm-granite/granite-embedding-small-english-r2
- https://huggingface.co/ibm-granite/granite-embedding-small-english-r2/raw/main/config.json
- https://huggingface.co/ibm-granite/granite-embedding-small-english-r2/raw/main/1_Pooling/config.json
- https://huggingface.co/ibm-granite/granite-embedding-small-english-r2/raw/main/sentence_bert_config.json
- https://huggingface.co/ibm-granite/granite-embedding-small-english-r2/raw/main/tokenizer_config.json
- https://huggingface.co/BAAI/bge-small-en-v1.5
- https://huggingface.co/BAAI/bge-small-en-v1.5/raw/main/1_Pooling/config.json
- https://huggingface.co/BAAI/bge-small-en-v1.5/raw/main/sentence_bert_config.json
- https://arxiv.org/html/2412.13663v2 (ModernBERT tokenizer details)
