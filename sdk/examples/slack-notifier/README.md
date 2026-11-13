# aegis-plugin-slack-notifier

**English** | [日本語](README.ja.md)

An AegisCMS sample plugin.
A minimal sample written in WAT that only receives the article save hook (`on-before-save`) and writes a log to the host (it does not actually send anything to Slack).

## Capabilities
- Network: `hooks.slack.com`
- Storage: `slack_notifier_state`
- Hooks: `on-before-save`, `transform-content-ast`
