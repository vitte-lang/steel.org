//! Steel CPython – Tests (MAX)
//!
//! Tests unitaires et de cohérence pour le backend CPython.
//!
//! Objectifs :
//! - vérifier PySpec (validation sémantique)
//! - vérifier PyArgs (argv généré)
//! - tester la détection Python de manière tolérante
//! - garantir la stabilité API driver/args
//!
//! Aucun test ne lance de build réel.

use super::args::{PyArgs, PyAction, PyBackend};
use super::spec::{PySpec, PyOutputKind};
use super::detect::{detect_python, PythonImpl};

/* ------------------------------------------------------------------------- */
/* PySpec                                                                    */
/* ------------------------------------------------------------------------- */

#[test]
fn pyspec_bytecode_minimal_ok() {
    let mut spec = PySpec::bytecode("bytecode-test");
    spec.add_source("main.py");

    assert!(spec.validate().is_ok());
    assert_eq!(spec.output_kind, PyOutputKind::Bytecode);
}

#[test]
fn pyspec_extension_requires_root() {
    let spec = PySpec::extension("ext-test", "proj");

    assert!(spec.validate().is_ok());
}

#[test]
fn pyspec_zipapp_requires_root_and_entry() {
    let spec = PySpec::zipapp("zip-test", "pkg", "pkg.__main__");

    assert!(spec.validate().is_ok());
}

#[test]
fn pyspec_nuitka_requires_entry() {
    let spec = PySpec::nuitka("native-test", "main.py");

    assert!(spec.validate().is_ok());
}

#[test]
fn pyspec_validation_fails_when_missing_inputs() {
    let spec = PySpec::bytecode("bad");

    assert!(spec.validate().is_err());
}

/* ------------------------------------------------------------------------- */
/* PyArgs                                                                    */
/* ------------------------------------------------------------------------- */

#[test]
fn pyargs_compile_bytecode_sources() {
    let mut args = PyArgs::new();
    args.action = PyAction::CompileBytecode;
    args.add_source("a.py");
    args.add_source("b.py");

    let argv = args.to_argv();

    assert!(argv.contains(&"-m".to_string()));
    assert!(argv.contains(&"compileall".to_string()));
    assert!(argv.contains(&"a.py".to_string()));
    assert!(argv.contains(&"b.py".to_string()));
}

#[test]
fn pyargs_compile_bytecode_root() {
    let mut args = PyArgs::new();
    args.action = PyAction::CompileBytecode;
    args.set_root("pkg");

    let argv = args.to_argv();

    assert!(argv.contains(&"pkg".to_string()));
}

#[test]
fn pyargs_build_extension() {
    let mut args = PyArgs::new();
    args.action = PyAction::BuildExtension;
    args.set_root("project");

    let argv = args.to_argv();

    assert!(argv.contains(&"setup.py".to_string()));
    assert!(argv.contains(&"build_ext".to_string()));
    assert!(argv.contains(&"--inplace".to_string()));
}

#[test]
fn pyargs_bundle_zipapp() {
    let mut args = PyArgs::new();
    args.action = PyAction::BundleZipapp;
    args.set_root("pkg");
    args.set_entry("pkg.__main__");
    args.set_output("app.pyz");

    let argv = args.to_argv();

    assert!(argv.contains(&"zipapp".to_string()));
    assert!(argv.contains(&"pkg".to_string()));
    assert!(argv.contains(&"-o".to_string()));
    assert!(argv.contains(&"app.pyz".to_string()));
}

#[test]
fn pyargs_bundle_nuitka() {
    let mut args = PyArgs::new();
    args.backend = PyBackend::Nuitka;
    args.action = PyAction::BundleNuitka;
    args.set_entry("main.py");
    args.set_output("dist");

    let argv = args.to_argv();

    assert!(argv.contains(&"nuitka".to_string()));
    assert!(argv.contains(&"--standalone".to_string()));
    assert!(argv.contains(&"main.py".to_string()));
}

/* ------------------------------------------------------------------------- */
/* Validation PyArgs                                                         */
/* ------------------------------------------------------------------------- */

#[test]
fn pyargs_validation_fails_without_sources_or_root() {
    let args = PyArgs::new();

    assert!(args.validate().is_err());
}

#[test]
fn pyargs_validation_ok_with_root() {
    let mut args = PyArgs::new();
    args.set_root("pkg");

    assert!(args.validate().is_ok());
}

/* ------------------------------------------------------------------------- */
/* Detect                                                                    */
/* ------------------------------------------------------------------------- */

#[test]
fn detect_python_best_effort() {
    match detect_python() {
        Ok(info) => {
            assert!(!info.version.is_empty());
            assert!(info.major >= 3);
            match info.implementation {
                PythonImpl::CPython | PythonImpl::PyPy | PythonImpl::Unknown => {}
            }
        }
        Err(_) => {
            // Environnement sans Python : test tolérant
            eprintln!("python not detected, skipping detect test");
        }
    }
}
