# ai-usage-dashboard

A unified terminal dashboard for AI provider usage — pulls **live data**
from z.ai, minimax.io, and OpenAI Codex, plus **historical token usage**
from your pi session logs.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ ai-usage-dashboard  ·  sorted by 5h % used  ·  refreshed 12s  ·  next 48s  │
├─────────────────────────────────────────────────────────────────────────────┤
│ tokens  all-time ─────────────────────────────────────────────────────────  │
│ model                msgs      input     output    cached      total  cost │
│ MiniMax-M3            158    157.6K     91.8K     19.55M     19.80M $2.66  │
├─────────────────────────────────────────────────────────────────────────────┤
│ OpenAI Codex  LIVE  chatgpt.com                                            │
│  5h   36.0%                                                          3h 26m│
│ ████████████████████████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│  Weekly  42.0%                                                       5d 19h│
├─────────────────────────────────────────────────────────────────────────────┤
│ minimax     LIVE  api.minimax.io                                           │
│  5h    5.5%                                                          4h 47m│
│  Weekly  15.5%                                                       6d 0h │
├─────────────────────────────────────────────────────────────────────────────┤
│ z.ai        ERROR api.z.ai                                                  │
│ HTTP 502 (body: )                                                          │
└─────────────────────────────────────────────────────────────────────────────┘
 [r] refresh  [q] quit  ·  auth: /home/sky/.pi/agent/auth.json
```

## Install

```sh
cargo install --git https://github.com/skylerahuman/ai-usage-dashboard
# or clone + install
git clone https://github.com/skylerahuman/ai-usage-dashboard
cd ai-usage-dashboard
cargo install --path .
```

Binary lands at `~/.cargo/bin/ai-usage-dashboard` (already on your PATH
if you have Rust set up).

## Run

```sh
ai-usage-dashboard              # interactive TUI
ai-usage-dashboard --once       # one-shot plain text (great for status bars)
ai-usage-dashboard --since 24h  # token window: all | 24h | 7d (default: all)
```

Keys: **`r`** refresh, **`q`/`Esc`** quit.

## What the bars mean

Bars show **% of quota used** (not remaining). Colors:

| range      | meaning                          |
| ---------- | -------------------------------- |
| 0–70%      | green — plenty of headroom       |
| 70–90%     | yellow — plan ahead              |
| 90–100%    | red — reset imminent             |

The right-aligned text on each row is the **time until that window resets**.

## Credentials

Auto-detected. Order: **env vars first, then `~/.pi/agent/auth.json`** (the
same file the `pi` coding agent uses), so if you use `pi` it usually Just
Works.

| provider        | env var                         | auth.json key            |
| --------------- | ------------------------------- | ------------------------ |
| z.ai            | `ZAI_API_KEY`                   | `zai-coding-plan.key`    |
| minimax         | `MINIMAX_CODING_PLAN_KEY`       | `minimax.key`            |
| OpenAI Codex    | `OPENAI_CODEX_OAUTH_TOKEN` + `OPENAI_CODEX_ACCOUNT_ID` | `openai-codex.access` + `openai-codex.accountId` |

For Codex the OAuth access token from your ChatGPT account is sufficient —
no Admin API key needed.

## Token panel (the new bit)

The **tokens** panel at the top of the dashboard reads your pi session
logs (`~/.pi/sessions/**/*.jsonl`) and aggregates token usage per model.
pi already records `input`, `output`, `cacheRead`, `cacheWrite`,
`totalTokens`, and `cost` per assistant message — no tokenizer needed.

Window: `all` (default), `24h`, or `7d`. The model name is mapped to a
provider heuristically (`glm-*` → z.ai, `minimax-*` → minimax, `gpt-*` →
Codex).

## Self-signed CA workaround

Some hosts (notably machines running `llmtrim`) have a broken system
trust store but ship their own CA at `~/.llmtrim/ca.pem` or pointed to by
`NODE_EXTRA_CA_CERTS`. The dashboard auto-loads the first CA it finds in:

1. `AI_USAGE_DASHBOARD_CA_FILE`
2. `~/.llmtrim/ca.pem`
3. `NODE_EXTRA_CA_CERTS`
4. `~/.config/llmtrim/ca.pem`
5. `~/.pi-trim/ca.pem`

Set `AI_USAGE_DASHBOARD_NO_EXTRA_CA=1` to disable.

## Architecture

```
src/
├── main.rs         # arg parsing, TUI event loop, --once mode
├── lib.rs          # exposes modules for integration tests
├── config.rs       # credential resolution (env → auth.json)
├── model.rs        # unified ProviderUsage / UsageWindow data model
├── aggregate.rs    # parallel fetch (tokio::join!) → Aggregated
├── tokens.rs       # pi session JSONL → per-model token summary
├── ui.rs           # ratatui rendering (header, tokens, providers, footer)
└── providers/
    ├── zai.rs      # GET https://api.z.ai/api/monitor/usage/quota/limit
    ├── minimax.rs  # GET https://api.minimax.io/v1/token_plan/remains
    └── codex.rs    # GET https://chatgpt.com/backend-api/wham/usage
```

## License

MIT. See [LICENSE](LICENSE).