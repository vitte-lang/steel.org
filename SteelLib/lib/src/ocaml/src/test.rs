//! Steel OCaml – Tests (MAX)
//!
//! Tests unitaires et de cohérence pour le backend OCaml.
//!
//! Objectifs :
//! - valider OcamlSpec (intention déclarative)
//! - valider OcamlArgs (argv déterministe)
//! - tester la détection OCaml de manière tolérante
//! - vérifier la compatibilité backend / capacités
//!
//! Aucun test ne lance de compilation réelle.

use std::path::PathBuf;

use super::args::{
    OcamlArgs, OcamlBackend, OcamlOptLevel, OcamlOutputKind,
};
use super::spec::OcamlSpec;
use super::detect::{detect_ocaml};

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

/* ------------------------------------------------------------------------- */
/* OcamlSpec                                                                 */
/* ------------------------------------------------------------------------- */

#[test]
fn ocamlspec_executable_minimal_ok() {
    let mut spec = OcamlSpec::executable("hello", "hello.exe");
    spec.add_source("main.ml");

    assert!(spec.validate().is_ok());
    assert_eq!(spec.output_kind, OcamlOutputKind::Executable);
}

#[test]
fn ocamlspec_library_ok() {
    let mut spec = OcamlSpec::library("mylib", "mylib.cmxa");
    spec.add_source("lib.ml");

    assert!(spec.validate().is_ok());
    assert_eq!(spec.output_kind, OcamlOutputKind::Library);
}

#[test]
fn ocamlspec_bytecode_backend() {
    let mut spec = OcamlSpec::bytecode("byte", "byte.byte");
    spec.add_source("main.ml");

    assert!(spec.validate().is_ok());
    assert_eq!(spec.backend, OcamlBackend::Ocamlc);
}

#[test]
fn ocamlspec_validation_fails_without_sources() {
    let spec = OcamlSpec::executable("bad", "bad.exe");

    assert!(spec.validate().is_err());
}

/* ------------------------------------------------------------------------- */
/* OcamlArgs                                                                 */
/* ------------------------------------------------------------------------- */

#[test]
fn ocamlargs_executable_native_basic() {
    let mut args = OcamlArgs::executable("app.exe");
    args.add_source("main.ml");

    let argv = args.to_argv();

    assert!(argv.contains(&"main.ml".to_string()));
    assert!(argv.contains(&"-o".to_string()));
    assert!(argv.contains(&"app.exe".to_string()));
}

#[test]
fn ocamlargs_library() {
    let mut args = OcamlArgs::library("lib.cmxa");
    args.add_source("lib.ml");

    let argv = args.to_argv();

    assert!(argv.contains(&"-a".to_string()));
    assert!(argv.contains(&"lib.cmxa".to_string()));
}

#[test]
fn ocamlargs_with_includes_and_libs() {
    let mut args = OcamlArgs::executable("app.exe");
    args.add_source("main.ml");
    args.add_include_dir("src");
    args.add_library("unix.cmxa");

    let argv = args.to_argv();

    assert!(argv.contains(&"-I".to_string()));
    assert!(argv.contains(&"src".to_string()));
    assert!(argv.contains(&"unix.cmxa".to_string()));
}

#[test]
fn ocamlargs_opt_level_o2() {
    let mut args = OcamlArgs::executable("app.exe");
    args.add_source("main.ml");
    args.set_opt_level(OcamlOptLevel::O2);

    let argv = args.to_argv();

    assert!(argv.contains(&"-O2".to_string()));
}

#[test]
fn ocamlargs_validation_fails_without_sources() {
    let args = OcamlArgs::executable("bad.exe");

    assert!(args.validate().is_err());
}

/* ------------------------------------------------------------------------- */
/* Detect                                                                    */
/* ------------------------------------------------------------------------- */

#[test]
fn detect_ocaml_best_effort() {
    match detect_ocaml() {
        Ok(info) => {
            assert!(!info.version.is_empty());
            assert!(info.has_bytecode || info.has_native);
        }
        Err(_) => {
            // Environnement sans OCaml : test tolérant
            eprintln!("OCaml not detected, skipping detect test");
        }
    }
}