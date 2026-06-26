# Development Commands

## Build
```
cargo build
cargo build --release
```

## Test
```
cargo test
cargo test <module_name>        # run tests for a specific module
cargo test --test <test_name>   # run a specific integration test
```

## Format
```
cargo fmt
cargo fmt -- --check            # check without modifying
```

## Lint
```
cargo clippy -- -D warnings
```

## Run
```
cargo run -- <input_dir> <output_dir> [--dry-run] [--verbose] [--jobs N] [--no-overwrite]
```
