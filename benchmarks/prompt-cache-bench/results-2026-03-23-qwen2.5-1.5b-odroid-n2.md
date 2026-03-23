# Prompt Cache Benchmark — Qwen2.5-1.5B-Instruct — Odroid N2 — 2026-03-23

## Setup

- **Model**: qwen2.5-1.5b-instruct-q5_k_m.gguf (pure transformer, 28 layers)
- **Hardware**: Odroid N2 (Amlogic S922X: 4x Cortex-A73 + 2x Cortex-A53, 4 GB RAM), CPU-only
- **Server**: `llama-server --parallel 1 -c 2048`
- **Endpoint**: `/completion` with `cache_prompt: true`, `id_slot: 0`
- **KV cache**: All 28 layers use standard attention — full prefix caching support

## Results — /completion with cache_prompt + id_slot

| # | Scenario | tokens_evaluated | tokens_cached | prompt_ms | predicted_ms | wall time |
|---|----------|-----------------|---------------|-----------|-------------|-----------|
| 1 | Cold — user A, first request | 275 | 306 | 31,237 ms | 6,193 ms | 37.5s |
| 2 | Warm — user B, different context | 243 | 255 | 3,820 ms | 2,276 ms | 6.1s |
| 3 | Warm — user A, different input | 272 | 289 | 7,235 ms | 3,341 ms | 10.6s |
| 4 | Hot — user A, exact repeat of #1 | 275 | 306 | **1,367 ms** | 6,080 ms | 7.5s |

## Analysis

1. **Cold start is very expensive**: 31s prompt eval on first request — the CPU
   must process all 275 tokens through all 28 layers with no cache.

2. **Prompt cache works**: Test 4 (exact repeat) drops prompt eval from 31.2s
   to **1.4s** — a **23x improvement**. The full KV cache is restored from the
   prompt cache, skipping almost all computation.

3. **Prefix reuse is partial**: Test 3 (same user, different input) shows 7.2s
   prompt eval — better than cold (31s) but not as fast as exact match (1.4s).
   The system prompt prefix is reused but the changed user message tokens still
   need evaluation.

4. **Context switching is costly**: Test 2 (different user context) takes 3.8s
   for prompt eval despite fewer tokens. The cache from test 1 is partially
   evicted, requiring re-evaluation of the different context tokens.

## Comparison with M1 Pro

| Scenario | Odroid N2 prompt_ms | M1 Pro prompt_ms | Ratio |
|----------|-------------------|------------------|-------|
| Cold | 31,237 ms | 290 ms | 108x |
| Same user, different input | 7,235 ms | 75 ms | 96x |
| Exact repeat | 1,367 ms | 51 ms | 27x |

The Odroid N2 is ~100x slower than M1 Pro for prompt evaluation (CPU-only vs
Metal GPU). The prompt cache narrows this gap significantly for repeat requests.

## Key Takeaway

On the Odroid N2, prompt caching is **critical** — it turns a 31s cold prompt
eval into a 1.4s cached one. For the typical single-user command bar scenario,
after the first cold request all subsequent requests benefit from the cached
system prompt prefix, bringing prompt eval down to 1-7s depending on how much
of the user message changes.
