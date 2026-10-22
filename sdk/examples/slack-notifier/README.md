# aegis-plugin-slack-notifier

AegisCMS サンプルプラグイン。
記事保存フック (`on-before-save`) を受信してホストにログを出すだけの、WAT で書いた最小のサンプルです（Slack への送信は実装していません）。

## ケイパビリティ
- Network: `hooks.slack.com`
- Storage: `slack_notifier_state`
- Hooks: `on-before-save`, `transform-content-ast`
