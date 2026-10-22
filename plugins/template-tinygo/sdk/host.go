package sdk

import "unsafe"

//go:wasmimport host log
func hostLog(level int32, ptr uintptr, len int32)

//go:wasmimport host http_check_allowed
func hostHttpCheckAllowed(ptr uintptr, len int32) int32

//go:wasmimport host kv_set
func hostKvSet(kPtr uintptr, kLen int32, vPtr uintptr, vLen int32) int32

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
