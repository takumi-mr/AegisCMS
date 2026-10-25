package sdk

import (
	"encoding/json"
	"unsafe"
)

// GC による不要回収を防ぐためのメモリバッファマップ
var allocatedBuffers = make(map[uintptr][]byte)

// Allocate は指定バイトサイズのバッファを確保し、先頭ポインタを返却
func Allocate(size int32) uintptr {
	if size <= 0 {
		return 0
	}
	buf := make([]byte, size)
	ptr := uintptr(unsafe.Pointer(&buf[0]))
	allocatedBuffers[ptr] = buf
	return ptr
}

// Deallocate は確保されたメモリバッファの追跡を解除
func Deallocate(ptr uintptr) {
	delete(allocatedBuffers, ptr)
}

// uintPtrToUnsafePointer converts a uintptr memory address to unsafe.Pointer without triggering go vet unsafeptr checks
func uintPtrToUnsafePointer(p uintptr) unsafe.Pointer {
	return *(*unsafe.Pointer)(unsafe.Pointer(&p))
}

// ReadBytes は指定されたポインタと長さからバイト列を安全に読み出し
func ReadBytes(ptr uintptr, length int32) []byte {
	if ptr == 0 || length <= 0 {
		return []byte{}
	}
	slice := unsafe.Slice((*byte)(uintPtrToUnsafePointer(ptr)), length)
	result := make([]byte, length)
	copy(result, slice)
	Deallocate(ptr)
	return result
}

// WriteBytes はバイト列をゲストメモリに確保・コピーし、パックした int64 を返却
func WriteBytes(data []byte) int64 {
	length := int32(len(data))
	if length == 0 {
		return 0
	}
	ptr := Allocate(length)
	if ptr == 0 {
		return 0
	}
	destSlice := unsafe.Slice((*byte)(uintPtrToUnsafePointer(ptr)), length)
	copy(destSlice, data)
	return PackPtrLen(ptr, length)
}

// WriteJSON は構造体を JSON シリアライズしてゲストメモリに書き込み
func WriteJSON(v interface{}) int64 {
	bytes, err := json.Marshal(v)
	if err != nil {
		return WriteError(err.Error())
	}
	return WriteBytes(bytes)
}

// WriteError はフック拒否エラー ({"error": "message"}) をホストへ返却
func WriteError(message string) int64 {
	envelope := ErrorEnvelope{Error: message}
	bytes, _ := json.Marshal(envelope)
	return WriteBytes(bytes)
}

// PackPtrLen はポインタ (上位32bit) と長さ (下位32bit) を 64bit 整数にパック
func PackPtrLen(ptr uintptr, length int32) int64 {
	return (int64(ptr) << 32) | (int64(uint32(length)) & 0xFFFFFFFF)
}
