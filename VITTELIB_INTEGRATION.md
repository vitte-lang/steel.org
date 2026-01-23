# VitteLib Integration

## Structure

```
vitteLib/
├── Cargo.toml                    # Package configuration
├── README                        # VitteLib documentation
├── lib/
│   ├── lib.rs                   # Library root
│   ├── fnmatch.rs               # Unix-like pattern matching
│   ├── fnmatch.in.rs            # Template variant
│   ├── glob.rs                  # Recursive glob patterns
│   └── glob.in.rs               # Template variant
└── m4/                          # Autoconf macros (legacy)
```

## Modules

### fnmatch — Pattern Matching (No Recursion)

POSIX-style filename pattern matching, typically used for single-level directory matching.

**Supported patterns:**
- `*` — Any sequence (except `/`)
- `?` — Any single character
- `[...]` — Character class
- `[!...]` — Negated class
- `\X` — Literal character

**Examples:**
```rust
use vittelib::fnmatch;

assert!(fnmatch::fnmatch("test.rs", "*.rs"));
assert!(fnmatch::fnmatch("file1.txt", "file[0-9].txt"));
assert!(!fnmatch::fnmatch("dir/test.rs", "*.rs")); // With pathname=true
```

**Options:**
- `pathname` — If true, `/` is not matched by wildcards (default: true)
- `period` — If true, `.` at start is not matched by `*` (default: false)
- `nocase` — Case-insensitive matching (default: false)

### glob — Recursive Patterns

Extended globbing with recursive directory matching using `**`.

**Supported patterns:**
- `*` — Any sequence (except `/`)
- `**` — Any sequence (including `/`)
- `?` — Any single character
- `[...]` — Character class
- `\X` — Literal character

**Examples:**
```rust
use vittelib::glob::GlobPattern;
use std::path::PathBuf;

let pattern = GlobPattern::new("src/**/*.rs").unwrap();

assert!(pattern.matches(&PathBuf::from("src/bin/steel.rs")));
assert!(pattern.matches(&PathBuf::from("src/utils/helpers.rs")));
assert!(!pattern.matches(&PathBuf::from("src/main.txt")));
```

**Options:**
- `nocase` — Case-insensitive matching (default: false)
- `no_hidden` — Skip files starting with `.` (default: false)

## Usage in Steel

### In src/bin/steel.rs

```rust
use vittelib::glob::GlobPattern;

fn find_sources(pattern: &str) -> Vec<PathBuf> {
    let glob = GlobPattern::new(pattern)?;
    // Use for scanning build artifacts, sources, etc.
}
```

### In build commands

```rust
use vittelib::fnmatch;

fn matches_target(filename: &str, pattern: &str) -> bool {
    fnmatch::fnmatch(filename, pattern)
}
```

## Workspace Configuration

VitteLib is integrated as a workspace member in the root `Cargo.toml`:

```toml
[workspace]
members = [".", "vitteLib"]
resolver = "2"

[dependencies]
vittelib = { path = "vitteLib" }
```

This allows:
- Single `cargo build` command for both packages
- Shared dependency versions
- Easier development and testing

## Building

### Build VitteLib only

```bash
cargo build --package vittelib
```

### Build with tests

```bash
cargo test --package vittelib
```

### Integration test

```bash
cargo test --test "*"
```

## Performance Considerations

Both modules use regex internally:

**fnmatch** — Single-level matching
- Fast for filename-only patterns
- Good for per-file filtering
- Complexity: O(n*m) where n=name length, m=pattern length

**glob** — Recursive matching with `**`
- Handles deeply nested patterns
- Still efficient with backtracking optimization
- Complexity: O(components * pattern_segments)

### Optimization tips

1. **Cache patterns** if using repeatedly:
   ```rust
   let pattern = GlobPattern::new("src/**/*.rs")?;
   for path in paths {
       if pattern.matches(&path) { /* ... */ }
   }
   ```

2. **Use fnmatch** for simple cases (faster)
   ```rust
   fnmatch("file.rs", "*.rs")  // Prefer over glob
   ```

3. **Use specific patterns** (more restrictive = faster)
   ```rust
   "src/**/*.rs"   // Good (specific)
   "**/*.rs"       // OK (less restrictive)
   "**/*"          // Slow (matches everything)
   ```

## Testing

VitteLib includes comprehensive tests:

```bash
# Run all tests
cargo test --package vittelib

# Run specific module tests
cargo test --package vittelib fnmatch
cargo test --package vittelib glob

# With output
cargo test --package vittelib -- --nocapture
```

Example test:
```rust
#[test]
fn test_glob_recursive() {
    let pattern = GlobPattern::new("src/**/*.rs").unwrap();
    assert!(pattern.matches(&PathBuf::from("src/bin/steel.rs")));
    assert!(pattern.matches(&PathBuf::from("src/a/b/c.rs")));
}
```

## Integration with Steel modules

### In parser/reader

```rust
use vittelib::glob::GlobPattern;

// Parse glob patterns from SteelConfig
pub fn expand_target_sources(pattern: &str) -> Result<Vec<PathBuf>> {
    let glob = GlobPattern::new(pattern)?;
    // Scan filesystem with glob
}
```

### In validator

```rust
use vittelib::fnmatch;

// Validate target names against patterns
pub fn is_valid_target(name: &str, constraints: &[&str]) -> bool {
    constraints.iter().any(|p| fnmatch::fnmatch(name, p))
}
```

### In resolver

```rust
use vittelib::glob::GlobPattern;

// Resolve dependencies based on glob patterns
pub fn get_matching_targets(pattern: &str, all_targets: &[Target]) -> Vec<Target> {
    let glob = GlobPattern::new(pattern)?;
    all_targets
        .iter()
        .filter(|t| glob.matches(Path::new(&t.name)))
        .cloned()
        .collect()
}
```

## Future enhancements

Possible additions:
- [ ] Negation patterns (`!pattern` to exclude)
- [ ] Performance optimizations for large filesets
- [ ] Caching of compiled regex patterns
- [ ] Alternative pattern syntaxes (gitignore-style, ant-style)
- [ ] Iterator-based directory traversal

## Dependencies

- `regex` — Pattern compilation and matching
- `anyhow` — Error handling

Both are already used by Steel core, so no additional bloat.
