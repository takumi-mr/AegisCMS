# AegisCMS TinyGo Plugin SDK

Official TinyGo / Go module for AegisCMS WebAssembly (Wasm) plugins.

[AegisCMS](https://github.com/takumi-mr/AegisCMS) provides zero-trust WebAssembly plugin capabilities. With TinyGo, you can write type-safe, lightweight, single-binary Wasm plugins using standard Go syntax and idioms.

## Installation

```bash
go get github.com/takumi-mr/AegisCMS/sdk/tinygo@v0.1.0
```

## Quick Start

### 1. `go.mod`

```go
module my-aegis-plugin

go 1.22

require github.com/takumi-mr/AegisCMS/sdk/tinygo v0.1.0
```

### 2. Implement Plugin (`main.go`)

```go
package main

import (
	"encoding/json"
	"strings"

	"github.com/takumi-mr/AegisCMS/sdk/tinygo"
)

//export allocate
func allocate(size int32) uintptr {
	return sdk.Allocate(size)
}

//export on_before_save
func on_before_save(ptr int32, length int32) int64 {
	inBytes := sdk.ReadBytes(uintptr(ptr), length)

	var post sdk.PostContext
	if err := json.Unmarshal(inBytes, &post); err != nil {
		sdk.LogError("[TinyGo] Failed to deserialize PostContext: " + err.Error())
		return sdk.WriteError("Invalid PostContext payload")
	}

	sdk.LogInfo("[TinyGo] Processing: " + post.Title)

	if strings.TrimSpace(post.Title) == "" {
		return sdk.WriteError("Post title cannot be empty")
	}

	// Persist state in plugin KV namespace
	sdk.KVSet("last_saved_slug", post.Slug)

	return sdk.WriteJSON(&post)
}

func main() {}
```

### 3. Build WebAssembly Binary

Compile with TinyGo (v0.31+ recommended):

```bash
tinygo build -target=wasm-unknown -opt=z -no-debug -o plugin.wasm main.go
```

Place `plugin.wasm` alongside `plugin.manifest.json` into `plugins/<name>/` in your AegisCMS installation.

## Supported Lifecycle Hooks

- `on_before_save(ptr int32, length int32) int64`
- `transform_content_ast(ptr int32, length int32) int64`
- `on_render_page(ptr int32, length int32) int64`

## Host APIs

- `sdk.LogDebug(msg)`, `sdk.LogInfo(msg)`, `sdk.LogWarn(msg)`, `sdk.LogError(msg)`
- `sdk.NetIsAllowed(url string) bool`
- `sdk.KVSet(key, value string) bool`

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
