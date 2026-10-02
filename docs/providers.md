# Zorai Providers

Zorai loads providers in two layers:

1. Built-in providers compiled into the daemon.
2. Additional custom providers from `custom-auth.yaml` in the Zorai runtime directory.

The daemon is the source of truth. TUI and React request the hydrated provider catalog from the daemon so model fetch, validation, main agents, and sub-agents all use the same provider definitions.

## Built-In Providers

| Provider | ID | Default model | Built-in models |
|---|---|---|---|
| Featherless | `featherless` | `meta-llama/Llama-3.3-70B-Instruct` | `meta-llama/Llama-3.3-70B-Instruct` |
| NVIDIA | `nvidia` | `minimaxai/minimax-m2.7` | `minimaxai/minimax-m2.7` |
| OpenAI | `openai` | `gpt-5.5` | `gpt-5.5`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.4-nano`, `gpt-5.3-codex`, `gpt-5.3-codex-spark`, `gpt-5.2-codex`, `gpt-5.2`, `gpt-5.1-codex-max`, `gpt-5.1-codex`, `gpt-5.1-codex-mini`, `gpt-5.1`, `gpt-5-codex`, `gpt-5-codex-mini`, `gpt-5`, `codex-mini-latest`, `o3`, `o4-mini`, `gpt-4.1`, `gpt-4.1-mini`, `gpt-4.1-nano`, `gpt-4o`, `gpt-4o-mini` |
| xAI | `xai` | `grok-4.3` | `grok-4.3`, `grok-4`, `grok-code-fast-1` |
| Azure OpenAI | `azure-openai` | user deployment | fetched from Azure `/models` |
| Anthropic | `anthropic` | `claude-opus-4-7` | `claude-opus-4-7`, `claude-opus-4-6`, `claude-opus-4-5-20251101`, `claude-opus-4-1-20250805`, `claude-opus-4-20250514`, `claude-sonnet-4-6`, `claude-sonnet-4-5-20250929`, `claude-sonnet-4-20250514`, `claude-3-7-sonnet-20250219`, `claude-haiku-4-5-20251001`, `claude-3-5-haiku-20241022`, `claude-3-opus-20240229`, `claude-3-haiku-20240307` |
| GitHub Copilot | `github-copilot` | `gpt-5.4` | `gpt-5.4`, `gpt-5.5`, `claude-haiku-4.5`, `claude-opus-4.5`, `claude-opus-4.6`, `claude-opus-4.6-fast`, `claude-opus-4.7`, `claude-sonnet-4`, `claude-sonnet-4.5`, `claude-sonnet-4.6`, `gemini-2.5-pro`, `gemini-3-flash-preview`, `gemini-3.1-pro-preview`, `gpt-4.1`, `gpt-4o`, `gpt-5-mini`, `gpt-5.1`, `gpt-5.1-codex`, `gpt-5.1-codex-max`, `gpt-5.1-codex-mini`, `gpt-5.2`, `gpt-5.2-codex`, `gpt-5.3-codex`, `gpt-5.4-mini`, `grok-code-fast-1`, `raptor-mini`, `goldeneye` |
| Qwen | `qwen` | `qwen-max` | `qwen-max`, `qwen-plus`, `qwen-turbo`, `qwen-long` |
| Qwen DeepInfra | `qwen-deepinfra` | `Qwen/Qwen2.5-72B-Instruct` | Qwen catalog defaults, remote fetch supported |
| Kimi | `kimi` | `moonshot-v1-32k` | `moonshot-v1-32k`, `moonshot-v1-8k`, `moonshot-v1-128k` |
| Kimi Coding Plan | `kimi-coding-plan` | `kimi-for-coding` | `kimi-k2.7-code`, `kimi-for-coding`, `kimi-k2.6`, `kimi-k2.5`, `kimi-k2-turbo-preview` |
| Z.AI | `z.ai` | `glm-4-plus` | `glm-4-plus`, `glm-5.1`, `glm-5`, `glm-4`, `glm-4-air`, `glm-4-flash` |
| Z.AI Coding Plan | `z.ai-coding-plan` | `glm-5` | `glm-5`, `glm-5.1`, `glm-4-plus`, `glm-4`, `glm-4-air`, `glm-4-flash` |
| Arcee | `arcee` | `trinity-large-thinking` | `trinity-large-thinking` |
| OpenRouter | `openrouter` | `arcee-ai/trinity-large-thinking` | `arcee/trinity-large-thinking`, `anthropic/claude-opus-4-6`, `openai/gpt-4.1`, `google/gemini-2.5-pro`, `meta-llama/llama-3.3-70b-instruct`; remote fetch supported |
| Cerebras | `cerebras` | `llama-3.3-70b` | `llama-3.3-70b` |
| Together | `together` | `meta-llama/Llama-3.3-70B-Instruct-Turbo` | `meta-llama/Llama-3.3-70B-Instruct-Turbo`, `deepseek-ai/DeepSeek-R1`, `Qwen/Qwen2.5-72B-Instruct-Turbo` |
| Groq | `groq` | `llama-3.3-70b-versatile` | `llama-3.3-70b-versatile`, `llama-3.1-8b-instant`, `gemma2-9b-it` |
| Ollama | `ollama` | `llama3.1` | `llama3.1`, `llama3.2`, `qwen2.5`, `codellama`; remote local fetch supported |
| Chutes | `chutes` | `deepseek-ai/DeepSeek-R1` | `deepseek-ai/DeepSeek-R1`; remote fetch supported |
| Hugging Face | `huggingface` | `meta-llama/Llama-3.3-70B-Instruct` | `meta-llama/Llama-3.3-70B-Instruct` |
| MiniMax | `minimax` | `MiniMax-M1-80k` | `MiniMax-M1-80k`, `MiniMax-M2.7`, `MiniMax-M2.5` |
| MiniMax Coding Plan | `minimax-coding-plan` | `MiniMax-M2.7` | `MiniMax-M1-80k`, `MiniMax-M2.7`, `MiniMax-M2.5` |
| Alibaba Coding Plan | `alibaba-coding-plan` | `qwen3.6-plus` | `qwen3.6-plus`, `qwen3-coder-plus`, `qwen3-coder-next`, `glm-5`, `kimi-k2.6`, `kimi-k2.5`, `MiniMax-M2.5` |
| Alibaba Token Plan | `alibaba-token-plan` | `qwen3.7-max` | `qwen3.7-max`, `qwen3.7-plus`, `qwen3.6-plus`, `qwen3.6-flash`, `qwen-image-2.0`, `qwen-image-2.0-pro`, `wan2.7-image`, `wan2.7-image-pro`, `deepseek-v4-pro`, `deepseek-v4-flash`, `deepseek-v3.2`, `kimi-k2.6`, `kimi-k2.5`, `glm-5.1`, `glm-5`, `MiniMax-M2.5` |
| Xiaomi MiMo | `xiaomi-mimo` | `mimo-v2.5-pro` | `mimo-v2.5-pro`, `mimo-v2.5-pro-ultraspeed`, `mimo-v2.5`, `mimo-v2.5-asr`, `mimo-v2.5-tts-voiceclone`, `mimo-v2.5-tts-voicedesign`, `mimo-v2.5-tts`, `mimo-v2-pro`, `mimo-v2-omni`, `mimo-v2-tts` |
| Xiaomi MiMo Token Plan | `xiaomi-mimo-token-plan` | `mimo-v2.5-pro` | `mimo-v2.5-pro`, `mimo-v2.5-pro-ultraspeed`, `mimo-v2.5`, `mimo-v2.5-asr`, `mimo-v2.5-tts-voiceclone`, `mimo-v2.5-tts-voicedesign`, `mimo-v2.5-tts`, `mimo-v2-pro`, `mimo-v2-omni`, `mimo-v2-tts` |
| Nous Portal | `nous-portal` | `nousresearch/hermes-4-70b` | `nousresearch/hermes-4-70b`, `nousresearch/hermes-4-405b`, `nousresearch/hermes-3-llama-3.1-70b`, `nousresearch/hermes-3-llama-3.1-405b` |
| OpenCode Zen | `opencode-zen` | `claude-sonnet-4-6` | `claude-opus-4-6`, `claude-sonnet-4-5`, `claude-sonnet-4`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.3-codex`, `minimax-m2.5`, `glm-5`, `kimi-k2.6`, `kimi-k2.5`; remote fetch supported |
| OpenCode Go | `opencode-go` | `glm-5.1` | `glm-5.1`, `glm-5`, `kimi-k2.7-code`, `kimi-k2.6`, `deepseek-v4-pro`, `deepseek-v4-flash`, `mimo-v2.5`, `mimo-v2.5-pro`, `minimax-m3`, `minimax-m2.7`, `minimax-m2.5`, `qwen3.7-max`, `qwen3.7-plus`, `qwen3.6-plus`; remote fetch supported |
| Cursor | `cursor` | `auto` | `auto`, `composer-2.5`, `grok-4.7`, `grok-4.6`, `grok-4.5`; uses the local `agent` CLI subscription login |
| Custom | `custom` | user-defined | user-defined single-provider fallback |

Providers marked with remote fetch support can load additional models from their `/models` endpoint when the provider exposes one.

## Custom Providers

For additional named custom providers, use [`custom-providers.md`](custom-providers.md).
