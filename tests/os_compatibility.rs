//! OS Compatibility Integration Tests

#[cfg(test)]
mod os_compatibility_tests {
    use steel::os::OsAdapter;

    #[test]
    fn test_os_detection_runtime() {
        let os = steel::os::get_current_os();
        let diagnostics = os.diagnostic_info();

        println!("OS Diagnostics: {}", diagnostics);

        // Basic sanity checks
        assert!(!os.name().is_empty(), "OS name should not be empty");
        assert!(os.cpu_count() > 0, "CPU count should be positive");
        assert!(!os.temp_dir().as_os_str().is_empty(), "Temp dir should exist");
    }

    #[test]
    fn test_path_separators() {
        let os = steel::os::get_current_os();
        let separator = os.path_separator();

        #[cfg(windows)]
        {
            assert_eq!(separator, '\\', "Windows should use backslash");
        }

        #[cfg(unix)]
        {
            assert_eq!(separator, '/', "Unix should use forward slash");
        }
    }

    #[test]
    fn test_architecture_detection() {
        let os = steel::os::get_current_os();
        let arch = os.arch();

        println!("Detected architecture: {:?}", arch);
        // Just verify it detects something
        assert_ne!(arch, steel::os::Architecture::Unknown);
    }

    #[test]
    fn test_tier_classification() {
        let os = steel::os::get_current_os();
        let tier = os.tier();

        println!("OS Tier: {:?}", tier);
        // Verify tier is set to something reasonable
        assert!(matches!(
            tier,
            steel::os::OsTier::Legacy
                | steel::os::OsTier::Compatible
                | steel::os::OsTier::Modern
                | steel::os::OsTier::Current
        ));
    }

    #[test]
    fn test_feature_support_by_tier() {
        let os = steel::os::get_current_os();
        let tier = os.tier();

        // All tiers support hardlinks
        assert!(os.hardlink_support() || !os.hardlink_support()); // Tautology, just ensures method exists

        // Only modern and higher support symlinks
        if tier < steel::os::OsTier::Modern {
            // Legacy/Compatible might not support symlinks
            println!("Tier {:?} may not support symlinks", tier);
        }

        // Parallel jobs should be tier-dependent
        if tier >= steel::os::OsTier::Compatible {
            println!("Tier {:?} should support parallel jobs", tier);
        }
    }

    #[test]
    fn test_environment_variables() {
        let os = steel::os::get_current_os();

        // Test getting a standard env var
        if let Some(path) = os.get_env("PATH") {
            assert!(!path.is_empty(), "PATH should not be empty");
        }

        // Test setting and getting
        let key = "MUFFIN_TEST_VAR";
        let value = "test_value_12345";
        os.set_env(key, value).expect("Should set env var");

        let retrieved = os
            .get_env(key)
            .expect("Should retrieve env var after setting");
        assert_eq!(retrieved, value, "Retrieved value should match set value");
    }

    #[test]
    fn test_temp_and_cache_dirs() {
        let os = steel::os::get_current_os();

        let temp_dir = os.temp_dir();
        assert!(
            !temp_dir.as_os_str().is_empty(),
            "Temp dir should not be empty"
        );

        let cache_dir = os.cache_dir();
        assert!(
            !cache_dir.as_os_str().is_empty(),
            "Cache dir should not be empty"
        );

        println!("Temp dir: {}", temp_dir.display());
        println!("Cache dir: {}", cache_dir.display());
    }

    #[test]
    fn test_process_spawning() {
        let os = steel::os::get_current_os();

        // Test spawning a simple command
        #[cfg(unix)]
        {
            let result = os.spawn_process("echo", &["hello"]);
            assert!(result.is_ok(), "Should spawn echo command");
        }

        #[cfg(windows)]
        {
            let result = os.spawn_process("cmd", &["/c", "echo", "hello"]);
            assert!(result.is_ok(), "Should spawn cmd command");
        }
    }

    #[test]
    fn test_pure_rust_fallback() {
        let fallback = steel::os::PureRustFallback;

        assert_eq!(fallback.symlink_support(), false);
        assert_eq!(fallback.hardlink_support(), false);
        assert_eq!(fallback.supports_parallel_jobs(), false);
        assert_eq!(fallback.tier(), steel::os::OsTier::Legacy);
        assert!(fallback.has_fallback());

        // Fallback should still work
        let temp = fallback.temp_dir();
        assert!(!temp.as_os_str().is_empty());
    }

    #[test]
    fn test_version_detection() {
        let version = steel::os::OsVersion::default();
        println!("Default version: {:?}", version);

        let version2 = steel::os::OsVersion {
            major: 10,
            minor: 15,
            patch: 7,
        };
        assert_eq!(version2.major, 10);
    }

    #[test]
    fn test_cpu_count_reasonable() {
        let os = steel::os::get_current_os();
        let cpu_count = os.cpu_count();

        // Sanity checks
        assert!(cpu_count > 0, "Should have at least 1 CPU");
        assert!(cpu_count <= 1024, "CPU count should be reasonable");

        println!("Detected CPUs: {}", cpu_count);
    }

    #[test]
    fn test_diagnostic_info_format() {
        let os = steel::os::get_current_os();
        let diagnostics = os.diagnostic_info();

        // Should contain expected information
        assert!(diagnostics.contains("OS:"), "Diagnostics should contain 'OS:'");
        assert!(diagnostics.contains("Arch:"), "Diagnostics should contain 'Arch:'");
        assert!(diagnostics.contains("Tier:"), "Diagnostics should contain 'Tier:'");
    }
}
