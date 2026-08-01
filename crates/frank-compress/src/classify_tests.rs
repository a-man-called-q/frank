#[cfg(test)]
mod tests {
    use crate::classify::*;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn known_code_filenames_win_over_any_extension_rule() {
        // Dockerfile has no extension; CMakeLists.txt would otherwise ride
        // the compressible .txt rule (#600 in the archive).
        assert_eq!(detect_file_type(Path::new("Dockerfile")), FileClass::Code);
        assert_eq!(detect_file_type(Path::new("dockerfile")), FileClass::Code);
        assert_eq!(detect_file_type(Path::new("Makefile")), FileClass::Code);
        assert_eq!(
            detect_file_type(Path::new("CMakeLists.txt")),
            FileClass::Code
        );
    }

    #[test]
    fn compressible_extensions_are_natural_language() {
        for ext in ["md", "txt", "markdown", "rst", "tex"] {
            assert_eq!(
                detect_file_type(Path::new(&format!("readme.{ext}"))),
                FileClass::NaturalLanguage,
                "{ext}"
            );
        }
    }

    #[test]
    fn code_extensions_are_code() {
        for ext in ["py", "js", "rs", "go", "sh", "sql"] {
            assert_eq!(
                detect_file_type(Path::new(&format!("f.{ext}"))),
                FileClass::Code,
                "{ext}"
            );
        }
    }

    #[test]
    fn config_extensions_are_config_not_code() {
        for ext in ["json", "yaml", "yml", "toml", "ini", "cfg", "env"] {
            assert_eq!(
                detect_file_type(Path::new(&format!("f.{ext}"))),
                FileClass::Config,
                "{ext}"
            );
        }
    }

    #[test]
    fn unknown_extension_is_unknown() {
        assert_eq!(detect_file_type(Path::new("f.xyz123")), FileClass::Unknown);
    }

    #[test]
    fn extensionless_shebang_file_is_code() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("myscript");
        std::fs::write(&p, "#!/usr/bin/env bash\necho hi\n").unwrap();
        assert_eq!(detect_file_type(&p), FileClass::Code);
    }

    #[test]
    fn extensionless_json_content_is_config() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("dotfile");
        std::fs::write(&p, r#"{"a": 1, "b": [1,2,3]}"#).unwrap();
        assert_eq!(detect_file_type(&p), FileClass::Config);
    }

    #[test]
    fn extensionless_yaml_content_is_config() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("dotfile");
        std::fs::write(
            &p,
            "name: test\nversion: 1.0\nsteps:\n  - run: echo hi\n  - run: echo bye\n",
        )
        .unwrap();
        assert_eq!(detect_file_type(&p), FileClass::Config);
    }

    #[test]
    fn extensionless_code_heavy_content_is_code() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("dotfile");
        std::fs::write(
            &p,
            "import os\nimport sys\nconst x = 1\nfunction foo() {\nif (x) {\nreturn x\n}\n}\n",
        )
        .unwrap();
        assert_eq!(detect_file_type(&p), FileClass::Code);
    }

    #[test]
    fn extensionless_prose_is_natural_language() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("CLAUDE");
        std::fs::write(&p, "This project is a Rust rebuild of Caveman.\nIt aims to be honest about token savings.\n").unwrap();
        assert_eq!(detect_file_type(&p), FileClass::NaturalLanguage);
    }

    #[test]
    fn should_compress_rejects_original_backup_siblings() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("notes.original.md");
        std::fs::write(&p, "some prose").unwrap();
        assert!(!should_compress(&p));
    }

    #[test]
    fn should_compress_rejects_directories() {
        let tmp = tempdir().unwrap();
        assert!(!should_compress(tmp.path()));
    }

    #[test]
    fn should_compress_true_for_markdown_file() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("notes.md");
        std::fs::write(&p, "some prose").unwrap();
        assert!(should_compress(&p));
    }

    #[test]
    fn should_compress_false_for_dockerfile() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("Dockerfile");
        std::fs::write(&p, "FROM rust:1.85\n").unwrap();
        assert!(!should_compress(&p));
    }
}
