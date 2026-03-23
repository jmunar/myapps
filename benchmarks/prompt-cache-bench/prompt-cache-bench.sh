#!/usr/bin/env bash
#
# Benchmark llama.cpp prompt caching for the command bar.
#
# Prerequisites:
#   llama-server -m models/Qwen3.5-2B-Q5_K_M.gguf --port 8081 --chat-template chatml --parallel 1
#
# Usage:
#   ./benchmarks/prompt-cache-bench.sh
#
set -euo pipefail

PORT="${LLAMA_PORT:-8081}"
BASE="http://127.0.0.1:${PORT}"

# ---------------------------------------------------------------------------
# Static system prompt (identical across all requests / users)
# ---------------------------------------------------------------------------
SYSTEM='Pick the action matching the user input. Reply JSON only.

Actions:
- leanfin.new_expense: Record a new expense [amount (number), description (text), category? (text)]
- leanfin.list_expenses: Show recent expenses [days? (number)]
- leanfin.delete_expense: Delete an expense by ID [id (number)]
- leanfin.summary: Show expense summary for a period [days? (number)]
- mindflow.capture_thought: Capture a new thought [content (text)]
- mindflow.list_thoughts: Show recent thoughts
- mindflow.delete_thought: Delete a thought [id (number)]
- voice_to_text.transcribe: Start a new transcription job
- voice_to_text.list_jobs: List transcription jobs
- classroom_input.new_input: Start a new input session [classroom (text), form_type (text)]
- classroom_input.delete_classroom: Delete a classroom [classroom (text)]
'

# ---------------------------------------------------------------------------
# Dynamic user messages (context + input, varies per user / request)
# ---------------------------------------------------------------------------
USER_A1='classroom_input.new_input: Available classrooms: Math 3A, Science 4B. Available form types: Quiz, Test
classroom_input.delete_classroom: Available classrooms: Math 3A, Science 4B
leanfin.new_expense: Available categories: Food, Transport, Entertainment

add 50 euros for groceries'

USER_B='classroom_input.new_input: Available classrooms: History 2B. Available form types: Exam
leanfin.new_expense: Available categories: Bills, Health

show my recent thoughts'

USER_A2='classroom_input.new_input: Available classrooms: Math 3A, Science 4B. Available form types: Quiz, Test
classroom_input.delete_classroom: Available classrooms: Math 3A, Science 4B
leanfin.new_expense: Available categories: Food, Transport, Entertainment

capture thought about meeting'

# ---------------------------------------------------------------------------
# JSON schema (constrains output to valid CommandIntent)
# ---------------------------------------------------------------------------
SCHEMA='{"type":"object","properties":{"action":{"type":"string","enum":["leanfin.new_expense","leanfin.list_expenses","leanfin.delete_expense","leanfin.summary","mindflow.capture_thought","mindflow.list_thoughts","mindflow.delete_thought","voice_to_text.transcribe","voice_to_text.list_jobs","classroom_input.new_input","classroom_input.delete_classroom"]},"params":{"type":"object"},"confidence":{"type":"number","minimum":0.0,"maximum":1.0}},"required":["action","params","confidence"]}'

# ---------------------------------------------------------------------------
# Helper: build chatml prompt
# ---------------------------------------------------------------------------
build_prompt() {
    local sys="$1" usr="$2"
    printf '<|im_start|>system\n%s<|im_end|>\n<|im_start|>user\n%s<|im_end|>\n<|im_start|>assistant\n' "$sys" "$usr"
}

# ---------------------------------------------------------------------------
# Helper: send a /completion request and print timing summary
# ---------------------------------------------------------------------------
run_request() {
    local label="$1" prompt="$2"
    echo "=== ${label} ==="
    time curl -s "${BASE}/completion" \
        -H "Content-Type: application/json" \
        -d "$(jq -n --arg prompt "$prompt" --argjson schema "$SCHEMA" '{
            prompt: $prompt,
            cache_prompt: true,
            id_slot: 0,
            temperature: 0.1,
            n_predict: 128,
            stop: ["<|im_end|>"],
            response_format: {type: "json_schema", json_schema: {schema: $schema}}
        }')" \
    | jq '{tokens_evaluated, tokens_cached, prompt_ms: .timings.prompt_ms, predicted_ms: .timings.predicted_ms, content: .content[0:120]}'
    echo
    sleep 1
}

# ---------------------------------------------------------------------------
# Wait for server
# ---------------------------------------------------------------------------
echo "Waiting for llama-server on port ${PORT}..."
for i in $(seq 1 30); do
    curl -s "${BASE}/health" >/dev/null 2>&1 && break
    sleep 1
done
echo

# ---------------------------------------------------------------------------
# Run benchmarks
# ---------------------------------------------------------------------------
PROMPT_A1=$(build_prompt "$SYSTEM" "$USER_A1")
PROMPT_B=$(build_prompt "$SYSTEM" "$USER_B")
PROMPT_A2=$(build_prompt "$SYSTEM" "$USER_A2")

run_request "1. Cold — user A, first request"          "$PROMPT_A1"
run_request "2. Warm — user B, different context"       "$PROMPT_B"
run_request "3. Warm — user A, different input"         "$PROMPT_A2"
run_request "4. Hot  — user A, exact repeat of #1"      "$PROMPT_A1"

echo "Done."
