package sdk

import (
	"encoding/json"
	"errors"
	"unsafe"
)

//go:wasmimport host log
func hostLog(level int32, ptr uintptr, len int32)

//go:wasmimport host http_check_allowed
func hostHttpCheckAllowed(ptr uintptr, len int32) int32

//go:wasmimport host kv_set
func hostKvSet(kPtr uintptr, kLen int32, vPtr uintptr, vLen int32) int32

//go:wasmimport host kv_get
func hostKvGet(kPtr uintptr, kLen int32) int64

//go:wasmimport host http_fetch
func hostHttpFetch(ptr uintptr, len int32) int64

//go:wasmimport host config_get
func hostConfigGet(kPtr uintptr, kLen int32) int64

// LogDebug はホストロガーへ DEBUG レベルのログを出力
func LogDebug(msg string) {
	bytes := []byte(msg)
	if len(bytes) == 0 {
		return
	}
	hostLog(0, uintptr(unsafe.Pointer(&bytes[0])), int32(len(bytes)))
}

// LogInfo はホストロガーへ INFO レベルのログを出力
func LogInfo(msg string) {
	bytes := []byte(msg)
	if len(bytes) == 0 {
		return
	}
	hostLog(1, uintptr(unsafe.Pointer(&bytes[0])), int32(len(bytes)))
}

// LogWarn はホストロガーへ WARN レベルのログを出力
func LogWarn(msg string) {
	bytes := []byte(msg)
	if len(bytes) == 0 {
		return
	}
	hostLog(2, uintptr(unsafe.Pointer(&bytes[0])), int32(len(bytes)))
}

// LogError はホストロガーへ ERROR レベルのログを出力
func LogError(msg string) {
	bytes := []byte(msg)
	if len(bytes) == 0 {
		return
	}
	hostLog(3, uintptr(unsafe.Pointer(&bytes[0])), int32(len(bytes)))
}

// NetIsAllowed は指定された URL への通信がマニフェストで許可されているか検証
func NetIsAllowed(url string) bool {
	bytes := []byte(url)
	if len(bytes) == 0 {
		return false
	}
	res := hostHttpCheckAllowed(uintptr(unsafe.Pointer(&bytes[0])), int32(len(bytes)))
	return res == 1
}

// KVSet はプラグイン専用ネームスペースの KVS へキーと値を永続化
func KVSet(key, value string) bool {
	kBytes := []byte(key)
	vBytes := []byte(value)
	if len(kBytes) == 0 {
		return false
	}
	var vPtr uintptr
	if len(vBytes) > 0 {
		vPtr = uintptr(unsafe.Pointer(&vBytes[0]))
	}
	res := hostKvSet(
		uintptr(unsafe.Pointer(&kBytes[0])),
		int32(len(kBytes)),
		vPtr,
		int32(len(vBytes)),
	)
	return res == 0
}

// takeHostBytes はホストが `allocate` で確保して書き込んだ結果 ((ptr << 32) | len) を読み出して領域を解放する。
// 0 以下 (値なし・エラー) なら nil を返す
func takeHostBytes(packed int64) []byte {
	if packed <= 0 {
		return nil
	}
	ptr := uintptr(packed >> 32)
	length := int32(packed & 0xFFFFFFFF)
	if ptr == 0 || length <= 0 {
		return nil
	}
	return ReadBytes(ptr, length)
}

// KVGet はプラグイン専用ネームスペースの KVS から値を読む。未設定なら found=false。
// マニフェストの capabilities.storage が必要 (未宣言・ホストのエラーは err)
func KVGet(key string) (value string, found bool, err error) {
	kBytes := []byte(key)
	if len(kBytes) == 0 {
		return "", false, nil
	}
	packed := hostKvGet(uintptr(unsafe.Pointer(&kBytes[0])), int32(len(kBytes)))
	switch {
	case packed == -1:
		return "", false, errors.New("KVS permission denied for key '" + key + "'")
	case packed < 0:
		return "", false, errors.New("KVS error for key '" + key + "'")
	}
	bytes := takeHostBytes(packed)
	if bytes == nil {
		return "", false, nil
	}
	return string(bytes), true, nil
}

// ConfigGet はサーバーの [plugins.settings.<プラグイン名>] の値を読む。未設定なら found=false。
// 文字列以外の値 (数値・配列など) は JSON 文字列で返る。空文字列も未設定として扱われる
func ConfigGet(key string) (value string, found bool) {
	kBytes := []byte(key)
	if len(kBytes) == 0 {
		return "", false
	}
	bytes := takeHostBytes(hostConfigGet(uintptr(unsafe.Pointer(&kBytes[0])), int32(len(kBytes))))
	if bytes == nil {
		return "", false
	}
	return string(bytes), true
}

// ConfigGetOr は ConfigGet の結果を返す。未設定なら fallback
func ConfigGetOr(key, fallback string) string {
	if value, found := ConfigGet(key); found {
		return value
	}
	return fallback
}

// HTTPRequest は HTTPFetch のリクエスト
type HTTPRequest struct {
	Method  string            `json:"method"`
	URL     string            `json:"url"`
	Headers map[string]string `json:"headers,omitempty"`
	Body    string            `json:"body,omitempty"`
}

// HTTPResponse は HTTPFetch の応答
type HTTPResponse struct {
	Status  int               `json:"status"`
	Headers map[string]string `json:"headers"`
	Body    string            `json:"body"`
}

// IsSuccess はステータスが 2xx かを返す
func (r *HTTPResponse) IsSuccess() bool {
	return r.Status >= 200 && r.Status < 300
}

// NewHTTPGet は GET リクエストを作る
func NewHTTPGet(url string) HTTPRequest {
	return HTTPRequest{Method: "GET", URL: url}
}

// NewHTTPPostJSON は JSON 本文の POST リクエスト (Content-Type: application/json) を作る
func NewHTTPPostJSON(url string, body interface{}) (HTTPRequest, error) {
	bytes, err := json.Marshal(body)
	if err != nil {
		return HTTPRequest{}, err
	}
	return HTTPRequest{
		Method:  "POST",
		URL:     url,
		Headers: map[string]string{"Content-Type": "application/json"},
		Body:    string(bytes),
	}, nil
}

// HTTPFetch は許可されたドメイン (capabilities.network.allowed_domains) へ HTTP リクエストを送る。
// リダイレクトはホストが 3 回まで、移った先ごとに許可を確かめて追う（ポートは 80・443・8080 のみ、内部の IP アドレスへは通信しない）。1 回のフック呼び出しで 8 回まで、本文は送受信とも 1 MiB まで。
// 通信の時間もフックの execution_timeout_ms に含まれる。通信の失敗・拒否は err
func HTTPFetch(request HTTPRequest) (*HTTPResponse, error) {
	body, err := json.Marshal(request)
	if err != nil {
		return nil, err
	}
	if len(body) == 0 {
		return nil, errors.New("empty http request")
	}
	packed := hostHttpFetch(uintptr(unsafe.Pointer(&body[0])), int32(len(body)))
	bytes := takeHostBytes(packed)
	if bytes == nil {
		return nil, errors.New("http_fetch returned nothing")
	}
	// 失敗時は {"error": "..."}。TinyGo の reflect は埋め込み構造体に弱いので平らな構造体で読む
	var result struct {
		Status  int               `json:"status"`
		Headers map[string]string `json:"headers"`
		Body    string            `json:"body"`
		Error   *string           `json:"error"`
	}
	if err := json.Unmarshal(bytes, &result); err != nil {
		return nil, err
	}
	if result.Error != nil {
		return nil, errors.New(*result.Error)
	}
	return &HTTPResponse{Status: result.Status, Headers: result.Headers, Body: result.Body}, nil
}
