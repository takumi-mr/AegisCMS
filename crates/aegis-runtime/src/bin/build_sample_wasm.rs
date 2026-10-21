fn main() {
    let wat_src = r#"
    (module
        (import "host" "log" (func $log (param i32 i32 i32)))
        (memory (export "memory") 1)
        (func (export "allocate") (param $len i32) (result i32)
            i32.const 64
        )
        (func (export "on_before_save") (param $ptr i32) (param $len i32) (result i64)
            ;; ホストログAPI (host::log) を呼び出してプラグインが動作したことを記録
            (call $log (i32.const 1) (local.get $ptr) (local.get $len))

            ;; 入力データをそのまま返す: (ptr << 32) | len
            (i64.or
                (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
                (i64.extend_i32_u (local.get $len))
            )
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat_src).expect("Failed to compile WAT");
    let out_path = std::path::Path::new("plugins/slack-notifier/plugin.wasm");
    std::fs::write(out_path, &wasm_bytes).expect("Failed to write plugin.wasm");
    println!(
        "Compiled and written {} bytes to {:?}",
        wasm_bytes.len(),
        out_path
    );
}
