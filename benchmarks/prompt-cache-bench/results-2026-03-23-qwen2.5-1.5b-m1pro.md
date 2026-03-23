# Prompt Cache Benchmark — Qwen2.5-1.5B-Instruct — 2026-03-23

## Setup

- **Model**: qwen2.5-1.5b-instruct-q5_k_m.gguf (pure transformer, 28 layers)
- **Hardware**: Apple M1 Pro, 16 GB RAM
- **Server**: `llama-server --parallel 1`
- **Endpoint**: `/completion` with `cache_prompt: true`, `id_slot: 0`
- **KV cache**: All 28 layers use standard attention — full prefix caching support

## Architecture

Qwen2.5-1.5B is a **pure decoder-only transformer**:
- 28 attention layers, all with KV cache (no SSM/Mamba layers)
- GQA with 12 query heads, 2 KV heads
- ChatML template native (`<|im_start|>` / `<|im_end|>`)
- KV buffer size: 896 MiB (f16)

## Results — /completion with cache_prompt + id_slot

| # | Scenario | tokens_evaluated | tokens_cached | prompt_ms | predicted_ms | wall time |
|---|----------|-----------------|---------------|-----------|-------------|-----------|
| 1 | Cold — user A, first request | 275 | 306 | 290 ms | 431 ms | 0.742s |
| 2 | Warm — user B, different context | 243 | 255 | 363 ms | 167 ms | 0.549s |
| 3 | Warm — user A, different input | 272 | 289 | **75 ms** | 238 ms | 0.335s |
| 4 | Hot — user A, exact repeat of #1 | 275 | 306 | **51 ms** | 429 ms | 0.502s |

## Comparison with Qwen3.5-2B (hybrid SSM + Attention)

| Scenario | Qwen3.5-2B prompt_ms | Qwen2.5-1.5B prompt_ms | Speedup |
|----------|---------------------|------------------------|---------|
| Cold | 384 ms | 290 ms | 1.3x |
| Same user, different input | 325 ms | **75 ms** | **4.3x** |
| Exact repeat | 324 ms | **51 ms** | **6.4x** |

## Key Findings

1. **Prefix caching works fully**: On the pure-transformer Qwen2.5, the entire
   static system prompt is cached and reused. Only the dynamic user message
   tokens need re-evaluation, reducing prompt eval from ~290ms to ~50-75ms.

2. **The hybrid architecture was the bottleneck**: Qwen3.5-2B's SSM layers
   forced full re-evaluation every time (~325ms). Qwen2.5-1.5B achieves 4-6x
   faster prompt eval on warm cache despite being a similar-size model.

3. **User context switching evicts cache**: When switching between users with
   different context (test 2), the cache is evicted and prompt eval is ~363ms.
   For a single-user scenario (typical for this app), subsequent requests after
   the first are consistently fast.

4. **JSON output quality**: The model correctly maps natural language to
   structured actions. With JSON schema constraint, output is well-formed.

## Additional Schema-Constrained Tests

| Scenario | prompt_ms | Output |
|----------|-----------|--------|
| Cold with schema | 96 ms | `{"action": "leanfin.new_expense", "amount": 50, ...}` |
| Warm with schema | **50 ms** | `{"action": "mindflow.capture_thought", "content": "about meeting"}` |
