# Apollo-client

[![Rustc Version](https://img.shields.io/badge/rustc-1.39+-lightgray.svg)](https://blog.rust-lang.org/2019/11/07/Rust-1.39.0.html)
[![Actions](https://github.com/jmjoy/apollo-client/workflows/Rust/badge.svg)](https://github.com/jmjoy/apollo-client/actions?query=workflow%3ARust)
[![Crate](https://img.shields.io/crates/v/apollo-client.svg)](https://crates.io/crates/apollo-client)
[![API](https://docs.rs/apollo-client/badge.svg)](https://docs.rs/apollo-client)

Rust🦀 client for Apollo.

Power by Rust `async/await`.

## Installation

With [cargo add](https://github.com/killercup/cargo-edit) installed run:

```sh
$ cargo add -s apollo-client
```

**Notice that the `xml` and `yaml` features aren't enable by default, if you have such type namespace, you should add 
`features` in `Cargo.toml`, just like:**

```toml
apollo-client = { version = "0.1.0", features = ["full"] }
```

## Usage

You can find some examples in [the examples directory](https://github.com/jmjoy/apollo-client/tree/master/examples).

## License

木兰宽松许可证, 第1版

