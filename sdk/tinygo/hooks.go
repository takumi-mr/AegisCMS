package sdk

import (
	"encoding/json"
	"errors"
)

// HookError はフックの失敗。Permanent が true なら再試行しても結果が変わらない失敗
// (send-notification でホストが再試行するかを決める)
type HookError struct {
	Message   string
	Permanent bool
}

func (e *HookError) Error() string { return e.Message }

// Reject は入力を受け付けないことを表す。メッセージはそのまま API の応答に出る
// (on-before-save は保存の拒否、calculate-order は注文の拒否、check-comment は投稿の拒否)。
// send-notification では再試行しない失敗として扱われる
func Reject(message string) error {
	return &HookError{Message: message, Permanent: true}
}

// IsPermanent は再試行しても同じ結果になる失敗かを返す。Reject 以外のエラーは再試行される
func IsPermanent(err error) bool {
	var hookErr *HookError
	return errors.As(err, &hookErr) && hookErr.Permanent
}

// RunHook はオブジェクトを受け取って返すフック (on-before-save / on-render-page / calculate-order など)
// の共通処理。入力を T に読み、f が書き換えた値を書き戻す。error は {"error": ...} (ホストは処理を断る)
func RunHook[T any](ptr int32, length int32, typeName string, f func(*T) error) int64 {
	var value T
	if err := json.Unmarshal(ReadBytes(uintptr(ptr), length), &value); err != nil {
		return WriteError("Invalid " + typeName + ": " + err.Error())
	}
	if err := f(&value); err != nil {
		return WriteError(err.Error())
	}
	return WriteJSON(&value)
}

// RunHookOut は入力と別の型を返すフック (check-comment の CommentVerdict など) の共通処理
func RunHookOut[I any, O any](ptr int32, length int32, typeName string, f func(*I) (O, error)) int64 {
	var input I
	if err := json.Unmarshal(ReadBytes(uintptr(ptr), length), &input); err != nil {
		return WriteError("Invalid " + typeName + ": " + err.Error())
	}
	output, err := f(&input)
	if err != nil {
		return WriteError(err.Error())
	}
	return WriteJSON(&output)
}

// RunNotificationHook は結果を返さない通知フック (on-order-status-changed / on-content-published)
// の共通処理。成功なら 0、error なら {"error": ...}
func RunNotificationHook[T any](ptr int32, length int32, typeName string, f func(*T) error) int64 {
	var value T
	if err := json.Unmarshal(ReadBytes(uintptr(ptr), length), &value); err != nil {
		return WriteError("Invalid " + typeName + ": " + err.Error())
	}
	if err := f(&value); err != nil {
		return WriteError(err.Error())
	}
	return 0
}

// RunCheckCommentHook はコメントの審査フック (check-comment) の共通処理
func RunCheckCommentHook(ptr int32, length int32, f func(*CommentSubmission) (CommentVerdict, error)) int64 {
	return RunHookOut(ptr, length, "CommentSubmission", f)
}

// RunDeliveryHook は通知の配信フック (send-notification) の共通処理。
// 送ったら 0、扱わない通知は {"skipped": true}、失敗は {"error": ..., "permanent": ...}
func RunDeliveryHook(ptr int32, length int32, f func(*NotificationMessage) (Delivery, error)) int64 {
	var message NotificationMessage
	if err := json.Unmarshal(ReadBytes(uintptr(ptr), length), &message); err != nil {
		return writeDeliveryError("Invalid NotificationMessage: "+err.Error(), true)
	}
	delivery, err := f(&message)
	if err != nil {
		return writeDeliveryError(err.Error(), IsPermanent(err))
	}
	if delivery == DeliverySkipped {
		return WriteBytes([]byte(`{"skipped":true}`))
	}
	return 0
}

func writeDeliveryError(message string, permanent bool) int64 {
	return WriteJSON(&struct {
		Error     string `json:"error"`
		Permanent bool   `json:"permanent"`
	}{message, permanent})
}

// HTML は render-blocks の結果に使う文字列へのポインタを返す (nil はそのブロックを出さない)
func HTML(html string) *string {
	return &html
}

// RunRenderBlocksHook はカスタムブロックの描画フック (render-blocks) の共通処理。
// f は入力と同じ数・同じ順の HTML を返す (nil はそのブロックを出さない)。
// error は {"error": ...} (そのプラグインのブロックは表示されない)
func RunRenderBlocksHook(ptr int32, length int32, f func([]BlockNode) ([]*string, error)) int64 {
	var input struct {
		Blocks []BlockNode `json:"blocks"`
	}
	if err := json.Unmarshal(ReadBytes(uintptr(ptr), length), &input); err != nil {
		return WriteError("Invalid render-blocks input: " + err.Error())
	}
	html, err := f(input.Blocks)
	if err != nil {
		return WriteError(err.Error())
	}
	if html == nil {
		html = []*string{}
	}
	return WriteJSON(&struct {
		HTML []*string `json:"html"`
	}{html})
}
