# ai-usage-dashboard

A unified terminal dashboard for AI provider usage — pulls live data from
**z.ai**, **MiniMax (minimax.io)**, and **OpenAI Codex**, and renders a
sorted-at-a-glance TUI in your terminal.

```
┌────────────────────────────────────────────────────────────────────────────┐
│ ai-usage-dashboard  ·  sorted by 5h % used  ·  refreshed 12s  ·  next 48s │
├────────────────────────────────────────────────────────────────────────────┤
│  OpenAI Codex LIVE  chatgpt.com                                           │
│  █████████████░░░░░░░░░░░░  5h       34.0%     resets in 3h 26m            │
│  ███████████████░░░░░░░░░  Weekly   41.0%     resets in 5d 19h            │
├────────────────────────────────────────────────────────────────────────────┤
│  minimax     LIVE  api.minimax.io                                         │
│  █░░░░░░░░░░░░░░░░░░░░░░░  5h       3.5%      resets in 4h 52m            │
│  █████░░░░░░░░░░░░░░░░░░░  Weekly   15.5%     resets in 6d 04h            │
├────────────────────────────────────────────────────────────────────────────┤
│  z.ai        ERROR api.z.ai                                                │
│  HTTP 502 (body: )                                                        │
└────────────────────────────────────────────────────────────────────────────┘
 [r] refresh  [q] quit  ·  auth: /home/sky/.pi/agent/auth.json
```

## Install

```sh
# from source (requires Rust 1.74+)
cargo install --git https://github.com/skylerahuman/ai-usage-dashboard

# or clone + install
git clone https://github.com/skylerahuman/ai-usage-dashboard
cd ai-usage-dashboard
cargo install --path .
```

## Run

```sh
ai-usage-dashboard          # interactive TUI
ai-usage-dashboard --once   # one-shot, plain text output
```

Keys: **`r`** to refresh now, **`q`** / **`Esc`** to quit.

## Credentials

The dashboard reads keys from **environment first**, then from
`~/.pi/agent/auth.json` (the same file the `pi` coding agent uses), so it
usually Just Works if you already use `pi`.

| provider        | env var                         | auth.json key            |
| --------------- | ------------------------------- | ------------------------ |
| z.ai            | `ZAI_API_KEY`                   | `zai-coding-plan.key`    |
| minimax         | `MINIMAX_CODING_PLAN_KEY`       | `minimax.key`            |
| OpenAI Codex    | `OPENAI_CODEX_OAUTH_TOKEN` + `OPENAI_CODEX_ACCOUNT_ID` | `openai-codex.access` + `openai-codex.accountId` |

For Codex, the OAuth access token from your ChatGPT account is sufficient —
no Admin API key needed.

## Self-signed CA workaround

Some hosts (notably machines running local LLM-helper tools like `llmtrim`)
have a broken system trust store but ship their own CA at
`~/.llmtrim/ca.pem` or pointed to by `NODE_EXTRA_CA_CERTS`. The dashboard
auto-loads the first CA it finds in:

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
├── ui.rs           # ratatui rendering (header, providers, footer)
└── providers/
    ├── zai.rs      # GET https://api.z.ai/api/monitor/usage/quota/limit
    ├── minimax.rs  # GET https://api.minimax.io/v1/token_plan/remains
    └── codex.rs    # GET https://chatgpt.com/backend-api/wham/usage
```

Sort key: **`5h used_percent` desc**. Providers without credentials are
shown as `NO AUTH`; transient failures (`502`, `429`, `parse`) are shown as
`ERROR` with the upstream message — they don't crash the dashboard.

## License

MIT. See [LICENSE](LICENSE).