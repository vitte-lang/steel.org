# Exemples Steel par langage

Ces dossiers contiennent des exemples complets, prets a lancer.
Chaque section donne des commandes "copier/coller/lancer" puis un petit extrait
de steelconf a reutiliser dans votre projet.

Prerequis:
- `steel` dans le PATH
- La toolchain du langage installee (cc, c++, python3, dotnet, go, javac, kotlinc,
  ocamlc, cargo/rustc, swiftc, zig)

## C
Copier/coller/lancer:
```
cd examples/c
steel run --root . --file steelconf --bake build_debug
steel run --root . --file steelconf --bake build_release
```
Extrait:
```
[tool cc]
  .exec "cc"
..

[bake app_c]
  .make c_src cglob "src/**/*.c"
  [run cc]
    .set "-O2" 1
    .set "-g" 1
    .takes c_src as "@args"
    .emits exe as "-o"
  ..
  .output exe "target/out/app_c"
..
```

## C++
Copier/coller/lancer:
```
cd examples/cpp
steel run --root . --file steelconf --bake build_debug
steel run --root . --file steelconf --bake build_release
```
Extrait:
```
[tool cxx]
  .exec "c++"
..

[bake app_cpp]
  .make cpp_src cglob "src/**/*.cpp"
  [run cxx]
    .set "-std=c++20" 1
    .set "-O2" 1
    .set "-g" 1
    .takes cpp_src as "@args"
    .emits exe as "-o"
  ..
  .output exe "target/out/app_cpp"
..
```

## Python
Copier/coller/lancer:
```
cd examples/cpython
steel run --root . --file steelconf --bake build
steel run --root . --file steelconf --bake test
```
Extrait:
```
[tool python]
  .exec "python3"
..

[bake python_run]
  .make py_src cglob "src/**/*.py"
  [run python]
    .set "-u" 1
    .set "-m" "src.main"
  ..
  .output exe "target/out/python.run"
..
```

## C#
Copier/coller/lancer:
```
cd examples/csharp
steel run --root . --file steelconf --bake build_debug
steel run --root . --file steelconf --bake build_release
```
Extrait:
```
[tool dotnet]
  .exec "dotnet"
..

[bake cs_build]
  .make csproj cglob "src/**/*.csproj"
  [run dotnet]
    .set "build" 1
    .takes csproj as "@args"
    .set "-c" "Release"
  ..
  .output exe "target/out/app_cs"
..
```

## Go
Copier/coller/lancer:
```
cd examples/go
steel run --root . --file steelconf --bake build_debug
steel run --root . --file steelconf --bake build_release
```
Extrait:
```
[tool go]
  .exec "go"
..

[bake go_build]
  .make go_src cglob "src/**/*.go"
  [run go]
    .set "build" 1
    .set "-o" "target/out/app_go"
    .set "./src" 1
  ..
  .output exe "target/out/app_go"
..
```

## Java
Copier/coller/lancer:
```
cd examples/java
steel run --root . --file steelconf --bake build
steel run --root . --file steelconf --bake test
```
Autres bakes dispo: `package`, `maven_package`, `gradle_build`.

Extrait:
```
[tool javac]
  .exec "javac"
..

[bake java_build]
  .make java_src cglob "src/**/*.java"
  [run javac]
    .set "-d" "target/classes"
    .takes java_src as "@args"
  ..
  .output classes "target/classes"
..
```

## Kotlin
Copier/coller/lancer:
```
cd examples/kotlin
steel run --root . --file steelconf --bake build_debug
steel run --root . --file steelconf --bake build_release
```
Extrait:
```
[tool kotlinc]
  .exec "kotlinc"
..

[bake kt_build]
  .make kt_src cglob "src/**/*.kt"
  [run kotlinc]
    .takes kt_src as "@args"
    .set "-d" "target/out/app.jar"
  ..
  .output jar "target/out/app.jar"
..
```

## OCaml
Copier/coller/lancer:
```
cd examples/ocaml
steel run --root . --file steelconf --bake build
steel run --root . --file steelconf --bake package
```
Extrait:
```
[tool ocamlc]
  .exec "ocamlc"
..

[bake ocaml_build]
  .make ml_src cglob "src/**/*.ml"
  [run ocamlc]
    .set "-g" 1
    .takes ml_src as "@args"
    .emits exe as "-o"
  ..
  .output exe "target/out/ocaml_app.byte"
..
```

## Rust
Copier/coller/lancer:
```
cd examples/rust
steel run --root . --file steelconf --bake build_debug
steel run --root . --file steelconf --bake build_release
```
Extrait:
```
[tool sh]
  .exec "sh"
..

[bake rust_build]
  .make rust_src cglob "src/**/*.rs"
  [run sh]
    .set "-c" "cargo build"
  ..
  .output exe "target/debug/app"
..
```

## Swift
Copier/coller/lancer:
```
cd examples/swift
steel run --root . --file steelconf --bake build_debug
steel run --root . --file steelconf --bake run_debug
```
Autres bakes dispo: `build_release`, `run_release`, `format`, `clean`.

Extrait:
```
[tool swiftc]
  .exec "swiftc"
..

[bake swift_build]
  .make swift_src cglob "Sources/**/*.swift"
  [run swiftc]
    .set "-g" 1
    .takes swift_src as "@args"
    .emits exe as "-o"
  ..
  .output exe "target/out/swift_app_debug"
..
```

## Zig
Copier/coller/lancer:
```
cd examples/zig
steel run --root . --file steelconf --bake build_debug
steel run --root . --file steelconf --bake build_release
```
Extrait:
```
[tool zig]
  .exec "zig"
..

[bake zig_build]
  .make zig_src cglob "src/**/*.zig"
  [run zig]
    .set "build-exe" 1
    .takes zig_src as "@args"
    .set "-O" "ReleaseFast"
  ..
  .output exe "target/out/app_zig"
..
```
