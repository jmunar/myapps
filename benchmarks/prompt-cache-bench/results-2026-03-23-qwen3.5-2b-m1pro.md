# Prompt Cache Benchmark — 2026-03-23

## Setup

- **Model**: Qwen3.5-2B-Q5_K_M.gguf (hybrid SSM + Attention, 24 layers)
- **Hardware**: Apple M1 Pro, 16 GB RAM
- **Server**: `llama-server --chat-template chatml --parallel 1`
- **Endpoint**: `/completion` with `cache_prompt: true`, `id_slot: 0`

## Architecture Note

Qwen3.5-2B is a **hybrid Mamba + Attention** model:
- 18 SSM (Mamba) layers — recurrent, must process all tokens every time
- 6 Attention layers (every 4th: 3, 7, 11, 15, 19, 23) — use KV cache, benefit from prefix reuse

This limits the effectiveness of prompt prefix caching: even when the KV cache
hits for the 6 attention layers, the 18 SSM layers still re-evaluate the full
sequence.

## Prompt Structure (new, optimised)

```
<|im_start|>system
{static action catalog — same for all users/requests}
<|im_end|>
<|im_start|>user
{per-user context lines}

{user input}
<|im_end|>
<|im_start|>assistant
```

The system block is 100% static across requests. Dynamic context and user input
are in the user block, so the cacheable prefix is maximised.

## Results — /completion endpoint (new)

| # | Scenario | tokens_evaluated | tokens_cached | prompt_ms | wall time |
|---|----------|-----------------|---------------|-----------|-----------|
| 1 | Cold — user A, first request | 285 | 340 | 384 ms | 1.355s |
| 2 | Warm — user B, different context | 253 | 274 | 290 ms | 0.676s |
| 3 | Warm — user A, different input | 282 | 409 | 325 ms | 2.456s |
| 4 | Hot — user A, exact repeat of #1 | 285 | 340 | 324 ms | 1.292s |

**Observation**: `tokens_evaluated` remains ~full prompt because SSM layers must
process every token regardless of KV cache hits. The KV cache reuse for the 6
attention layers provides a modest ~15-20% improvement in prompt eval time after
the first cold request.

## Comparison — /v1/chat/completions (old)

| # | Scenario | prompt_tokens | cached_tokens | wall time |
|---|----------|--------------|---------------|-----------|
| 1 | Cold | 277 | 0 | 1.866s |
| 2 | Same prefix, different input | 276 | 0 | 1.033s |
| 3 | Exact repeat of #2 | 276 | 272 | 0.696s |

The old `/v1/chat/completions` endpoint only cached exact-match requests (test 3)
and never reused prefix tokens across requests with different suffixes.

## Key Findings

1. **Hybrid SSM models limit prefix caching**: With only 6/24 layers using KV
   cache, the maximum theoretical speedup from prefix caching is ~25%.

2. **`/completion` > `/v1/chat/completions` for caching**: The raw endpoint with
   `cache_prompt: true` + `id_slot: 0` enables per-slot KV cache reuse. The
   chat completions endpoint does not support prefix reuse at all.

3. **Prompt restructuring matters**: Moving dynamic context from the system
   message to the user message ensures the static prefix is byte-identical
   across all requests, maximising cache hit rate.

4. **For significantly better caching**: Consider a pure-transformer model
   (e.g. Qwen2.5-1.5B-Instruct) where all 28 layers use KV cache and prefix
   reuse would skip re-evaluation of the entire static prefix.
