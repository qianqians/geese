# consulrs_derive (vendored)

上游：https://github.com/jmgilman/consulrs ，crate `consulrs_derive` v0.1.0（MIT）。
该 crate 只有 0.1.0 一个版本，且已不再维护，所以这里**原样拷贝 + 一处修补**，
通过根 `Cargo.toml` 的 `[patch.crates-io]` 覆盖，避免 cargo 从 registry 拉原版。

## 为什么修补

原版 `src/error.rs` 里用了 `quote_spanned!`，但**没有从 `quote` 引入**，而是靠
`lib.rs` 的 `#[macro_use] extern crate synstructure;` 间接拿到这个宏。
Rust 现在对"私有宏跨 crate 使用"报未来不兼容警告（rust-lang/rust#120192）：

```
warning: macro `quote_spanned` is private
  --> consulrs_derive-0.1.0/src/error.rs:17:15
  = warning: this was previously accepted by the compiler but is being phased out;
    it will become a hard error in a future release!
```

## 修补内容（只有一行）

`src/error.rs` 里加：

```rust
use quote::quote_spanned;
```

`quote` 本来就是本 crate 的直接依赖，所以不需要动 Cargo.toml，行为完全不变。
