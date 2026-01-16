# Toolchain Detection (CPython / OCaml / GHC)

This document describes how Steel detects and validates language toolchains
for the CPython, OCaml, and GHC Haskell backends. It lists the expected
executables, the environment variables you can use, and the typical failure
modes.

## 1. CPython / PyPy / Nuitka

### Executables
- `python3` (preferred)
- `python` (fallback)
- `pypy3` (only if you explicitly request the PyPy backend)
- `nuitka` (only if you explicitly request the Nuitka backend)

### Detection strategy
- Steel probes `python3` then `python` using an inline script:
  - `platform.python_implementation()`
  - `sys.version_info`
- The detected implementation is one of:
  - `CPython`
  - `PyPy`
  - `Unknown` (non-standard implementation)

### Environment variables (recommended)
- `PYTHON` or `PYTHONBIN`: point to a specific interpreter if multiple are
  installed (set this in your shell or wrapper script and pass it to Steel).
- `PYTHONPATH`: if you rely on non-standard module paths at build time.
- `PYTHONUTF8=1`: enforce UTF-8 in environments with legacy locale defaults.
- `NUITKA_CACHE_DIR`: set if you want Nuitka to write cache outside project.

### Typical failures
- "no compatible python interpreter found in PATH":
  - `python3`/`python` not installed or not on `PATH`.
- "CPython backend requested but interpreter is not CPython":
  - You requested CPython but `python` points to PyPy.
- "Nuitka backend requires CPython interpreter":
  - `python` is not CPython, or Nuitka is not installed.

### Suggested checks
```
python3 -c "import platform,sys;print(platform.python_implementation(),sys.version)"
python -c "import platform,sys;print(platform.python_implementation(),sys.version)"
python -m nuitka --version
```

## 2. OCaml (ocamlc / ocamlopt)

### Executables
- `ocamlc` (bytecode)
- `ocamlopt` (native)

### Detection strategy
- Steel checks `ocamlc` and `ocamlopt` availability in `PATH`.
- Backend selection:
  - `Ocamlc` requires `ocamlc`.
  - `Ocamlopt` requires `ocamlopt`.

### Environment variables (recommended)
- `OCAMLPATH`: required if your build relies on custom OCaml libs.
- `OCAMLLIB`: override the standard library location if needed.
- `CAML_LD_LIBRARY_PATH`: runtime lookup for compiled libs during tests.

### Typical failures
- "ocamlc requested but not available":
  - `ocamlc` missing or not on `PATH`.
- "ocamlopt requested but not available":
  - `ocamlopt` missing or not on `PATH`.

### Suggested checks
```
ocamlc -version
ocamlopt -version
ocamlc -where
```

## 3. GHC Haskell

### Executables
- `ghc` (required)

### Detection strategy
- Steel runs `ghc --print-libdir` to verify presence.
- Then queries `ghc --numeric-version`.
- Both native and bytecode modes are considered supported by default; the
  detection step only verifies `ghc` can run.

### Environment variables (recommended)
- `GHC_PACKAGE_PATH`: if you use a custom package database.
- `GHC_ENVIRONMENT`: select a specific environment file.
- `PATH`: must include your `ghc` installation.

### Typical failures
- "ghc not found in PATH":
  - `ghc` is missing or not on `PATH`.
- "ghc executable found but unusable":
  - `ghc` exists but fails to run (permissions, broken install).

### Suggested checks
```
ghc --version
ghc --numeric-version
ghc --print-libdir
```

## 4. Troubleshooting checklist

1) Verify the tool binary is on `PATH`.
2) Check the version output matches what your project expects.
3) If you use virtual environments or custom installs, export the related
   environment variables before running Steel.
4) Use `which <tool>` / `where <tool>` to confirm the resolved executable.
5) Run the suggested checks above to validate the toolchain manually.
