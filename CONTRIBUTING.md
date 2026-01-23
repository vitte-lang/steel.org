You can add it automatically with:
- `git commit -s`

By submitting a PR, you certify the DCO (https://developercertificate.org/).

## Testing

Run the default checks before submitting:
- build: `cargo build` (or your project build command)
- tests: `cargo test`
- formatting: `cargo fmt` (if Rust) and/or project formatters
- docs/manpage: ensure `steel.1` renders with `man ./doc/steel.1`

If the repository uses Steel to build itself, also run:
- `steel build`
- `steel test`

## License

Unless explicitly stated otherwise, contributions are licensed under the Apache License 2.0 (see `LICENSE`).

By contributing, you agree that your contribution may be redistributed under the project license.

## Security

If you believe you have found a security issue:
- do not open a public issue with exploit details
- contact the maintainers privately (define the channel in SECURITY.md if present)

## Code of Conduct

Be professional and constructive. If a `CODE_OF_CONDUCT.md` exists, it applies.