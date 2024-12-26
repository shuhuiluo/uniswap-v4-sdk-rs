# Uniswap V4 SDK Rust

[![Rust CI](https://github.com/shuhuiluo/uniswap-v4-sdk-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/shuhuiluo/uniswap-v4-sdk-rs/actions/workflows/rust.yml)
[![docs.rs](https://img.shields.io/docsrs/uniswap-v4-sdk)](https://docs.rs/uniswap-v4-sdk/latest/uniswap_v4_sdk/)
[![crates.io](https://img.shields.io/crates/v/uniswap-v4-sdk.svg)](https://crates.io/crates/uniswap-v4-sdk)

A Rust SDK for building applications on top of Uniswap V4. Rewrite of the
TypeScript [V4 SDK](https://github.com/Uniswap/sdks).

It is feature-complete but missing unit tests.

## Features

- Opinionated Rust implementation of the Uniswap V4 SDK with a focus on readability and performance
- Usage of [alloy-rs](https://github.com/alloy-rs) types
- Consistent API and types with the [V3 SDK](https://github.com/shuhuiluo/uniswap-v3-sdk-rs)
  and [SDK Core](https://github.com/malik672/uniswap-sdk-core-rust)

## Note on `no_std`

By default, this library does not depend on the standard library (`std`). However, the `std` feature can be enabled.

## Contributing

Contributions are welcome. Please open an issue if you have any questions or suggestions.

### Testing

Tests are run with

```shell
cargo test
```

### Linting

Linting is done with `clippy` and `rustfmt`. To run the linter, use:

```shell
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

## License

This project is licensed under the [MIT License](LICENSE).

## Acknowledgements

This project is inspired by and adapted from the following projects:

- [Uniswap V4 SDK](https://github.com/Uniswap/sdks)
- [Uniswap V3 SDK Rust](https://github.com/shuhuiluo/uniswap-v3-sdk-rs)
- [Uniswap SDK Core Rust](https://github.com/malik672/uniswap-sdk-core-rust)
