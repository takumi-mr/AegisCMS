# aegis-plugin-slack-notifier

AegisCMS サンプルプラグイン。
記事保存フック (`on-before-save`) を受信し、Slack Webhook へ更新通知を送信します。

## ケイパビリティ
- Network: `hooks.slack.com`
- Storage: `slack_notifier_state`
- Hooks: `on-before-save`, `transform-content-ast`
