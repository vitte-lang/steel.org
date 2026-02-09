#![cfg(test)]

use std::path::PathBuf;

use super::args::{CStd, GccArgs, GccMode};
use super::detect::{CcKind};
use super::spec::{CSpec, CSpecOverrides};

#[test]
fn detect_classify_compiler_text() {
    // classification sans exécuter de tool réel
    fn classify(s: &str) -> CcKind {
        let v = s.to_ascii_lowercase();
        if v.contains("clang") {
            CcKind::Clang
        } else if v.contains("gcc")
            || v.contains("gnu compiler")
            || v.contains("free software foundation")
        {
            CcKind::Gcc
        } else {
            CcKind::Unknown
        }
    }

    assert_eq!(classify("clang version 17.0.6 (....)"), CcKind::Clang);
    assert_eq!(classify("gcc (GCC) 13.2.0"), CcKind::Gcc);
    assert_eq!(classify("Apple clang version 15.0.0"), CcKind::Clang);
    assert_eq!(classify("Some Weird C Compiler 1.0"), CcKind::Unknown);
}

#[test]
fn args_compile_basic_flags() {
    let mut a = GccArgs::compile();
    a.c_std = Some(CStd::C17);
    a.debug = true;
    a.opt_level = Some(0);
    a.warnings = vec!["-Wall".into(), "-Wextra".into()];
    a.includes.push(PathBuf::from("include"));
    a.defines.push(("DEBUG".into(), None));
    a.input = Some(PathBuf::from("src/main.c"));
    a.output = Some(PathBuf::from("target/obj/main.o"));
    a.depfile = Some(PathBuf::from("target/dep/main.d"));

    let argv = a.build_args();
    let s: Vec<String> = argv.iter().map(|x| x.to_string_lossy().to_string()).collect();

    // invariants
    assert!(s.contains(&"-c".to_string()));
    assert!(s.contains(&"-std=c17".to_string()));
    assert!(s.contains(&"-g".to_string()));
    assert!(s.contains(&"-O0".to_string()));
    assert!(s.contains(&"-Wall".to_string()));
    assert!(s.contains(&"-Wextra".to_string()));
    assert!(s.contains(&"-I".to_string()));
    assert!(s.contains(&"include".to_string()));
    assert!(s.iter().any(|x| x.starts_with("-DDEBUG")));
    assert!(s.contains(&"-MMD".to_string()));
    assert!(s.contains(&"-MF".to_string()));
    assert!(s.contains(&"target/dep/main.d".to_string()));
    assert!(s.contains(&"-o".to_string()));
    assert!(s.contains(&"target/obj/main.o".to_string()));
    assert!(s.contains(&"src/main.c".to_string()));

    // mode compile doit être bien set
    assert_eq!(a.mode, GccMode::Compile);
}

#[test]
fn args_link_basic_flags_and_inputs() {
    let mut a = GccArgs::link();
    a.opt_level = Some(2);
    a.lto = true;
    a.lib_dirs.push(PathBuf::from("target/lib"));
    a.libs.push("m".into());
    a.link_inputs.push(PathBuf::from("target/obj/a.o"));
    a.link_inputs.push(PathBuf::from("target/obj/b.o"));
    a.output = Some(PathBuf::from("target/bin/app.exe"));

    let argv = a.build_args();
    let s: Vec<String> = argv.iter().map(|x| x.to_string_lossy().to_string()).collect();

    // invariants
    assert!(!s.contains(&"-c".to_string()));
    assert!(s.contains(&"-O2".to_string()));
    assert!(s.contains(&"-flto".to_string()));
    assert!(s.contains(&"-L".to_string()));
    assert!(s.contains(&"target/lib".to_string()));
    assert!(s.contains(&"-lm".to_string()));
    assert!(s.contains(&"target/obj/a.o".to_string()));
    assert!(s.contains(&"target/obj/b.o".to_string()));
    assert!(s.contains(&"-o".to_string()));
    assert!(s.contains(&"target/bin/app.exe".to_string()));

    assert_eq!(a.mode, GccMode::Link);
}

#[test]
fn spec_dev_release_profiles() {
    let dev = CSpec::dev();
    assert!(dev.debug);
    assert_eq!(dev.opt_level, 0);

    let rel = CSpec::release();
    assert!(!rel.debug);
    assert!(rel.opt_level >= 2);
    assert!(rel.lto);
    assert!(rel.pic); // dans la spec proposée
}

#[test]
fn spec_apply_overrides() {
    let mut s = CSpec::dev();

    let mut o = CSpecOverrides::default();
    o.debug = Some(false);
    o.opt_level = Some(3);
    o.lto = Some(true);
    o.werror = Some(true);
    o.warnings.push("-Wshadow".into());
    o.includes.push(PathBuf::from("inc"));
    o.defines.push(("FOO".into(), Some("1".into())));

    s.apply_overrides(o);

    assert!(!s.debug);
    assert_eq!(s.opt_level, 3);
    assert!(s.lto);
    assert!(s.werror);
    assert!(s.warnings.iter().any(|w| w == "-Wshadow"));
    assert!(s.includes.iter().any(|p| p == &PathBuf::from("inc")));
    assert!(s.defines.iter().any(|(k, v)| k == "FOO" && v.as_deref() == Some("1")));
}

/// Test d’intégration optionnel: requiert un compilateur dans PATH.
/// Lance-le explicitement:
///   cargo test -p SteelLib gcc_detect_real_tool -- --ignored --nocapture
#[test]
#[ignore]
fn gcc_detect_real_tool() {
    let tool = super::detect::detect_cc().expect("should detect gcc/clang");
    eprintln!("detected: exe={:?} kind={:?} target={:?}", tool.exe, tool.kind, tool.target_triple);
    assert!(tool.version_text.len() > 0);
}
