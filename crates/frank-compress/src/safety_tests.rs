#[cfg(test)]
mod tests {
    use crate::backup::{backup_dir_for, backup_path_for};
    use crate::frontmatter::split_frontmatter;
    use crate::sensitive::is_sensitive_path;
    use std::path::Path;

    #[test]
    fn refuses_dotenv_and_variants() {
        assert!(is_sensitive_path(Path::new(".env")));
        assert!(is_sensitive_path(Path::new(".env.production")));
        assert!(is_sensitive_path(Path::new(".netrc")));
    }

    #[test]
    fn refuses_credentials_and_secrets_basenames() {
        assert!(is_sensitive_path(Path::new("credentials.json")));
        assert!(is_sensitive_path(Path::new("secrets.yaml")));
        assert!(is_sensitive_path(Path::new("secret.md")));
        assert!(is_sensitive_path(Path::new("passwords.txt")));
    }

    #[test]
    fn refuses_ssh_key_files() {
        assert!(is_sensitive_path(Path::new("id_rsa")));
        assert!(is_sensitive_path(Path::new("id_ed25519.pub")));
        assert!(is_sensitive_path(Path::new("authorized_keys")));
        assert!(is_sensitive_path(Path::new("known_hosts")));
    }

    #[test]
    fn refuses_by_extension() {
        for ext in [
            "pem", "key", "p12", "pfx", "crt", "cer", "jks", "keystore", "asc", "gpg",
        ] {
            assert!(
                is_sensitive_path(Path::new(&format!("thing.{ext}"))),
                "{ext}"
            );
        }
    }

    #[test]
    fn refuses_files_inside_sensitive_directories() {
        assert!(is_sensitive_path(Path::new("/home/user/.ssh/notes.md")));
        assert!(is_sensitive_path(Path::new("/home/user/.aws/readme.md")));
        assert!(is_sensitive_path(Path::new("/home/user/.gnupg/todo.md")));
    }

    #[test]
    fn refuses_apikey_regardless_of_separator_style() {
        assert!(is_sensitive_path(Path::new("api-key.md")));
        assert!(is_sensitive_path(Path::new("api_key.md")));
        assert!(is_sensitive_path(Path::new("API KEY.md")));
    }

    #[test]
    fn ordinary_markdown_is_not_sensitive() {
        assert!(!is_sensitive_path(Path::new("README.md")));
        assert!(!is_sensitive_path(Path::new("/home/user/notes/todo.md")));
        assert!(!is_sensitive_path(Path::new("CLAUDE.md")));
    }

    #[test]
    fn backup_dir_mirrors_source_parent_name() {
        let dir = backup_dir_for(Path::new("/repo/notes/task.md"));
        assert!(dir.ends_with("notes"));
        assert!(dir.to_string_lossy().contains("frank-compress"));
        assert!(dir.to_string_lossy().contains("backups"));
    }

    #[test]
    fn backup_path_uses_original_md_suffix() {
        let p = backup_path_for(Path::new("/repo/notes/task.md"));
        assert_eq!(p.file_name().unwrap(), "task.original.md");
    }

    #[test]
    fn splits_frontmatter_when_present() {
        let text = "---\ntitle: x\n---\nBody text here.";
        let (fm, body) = split_frontmatter(text);
        assert_eq!(fm, "---\ntitle: x\n---\n");
        assert_eq!(body, "Body text here.");
        assert_eq!(
            format!("{fm}{body}"),
            text,
            "frontmatter + body must reassemble byte-exactly"
        );
    }

    #[test]
    fn no_frontmatter_returns_whole_text_as_body() {
        let text = "Just plain prose, no frontmatter.";
        let (fm, body) = split_frontmatter(text);
        assert_eq!(fm, "");
        assert_eq!(body, text);
    }

    #[test]
    fn frontmatter_with_crlf_line_endings() {
        let text = "---\r\ntitle: x\r\n---\r\nBody.";
        let (fm, body) = split_frontmatter(text);
        assert_eq!(fm, "---\r\ntitle: x\r\n---\r\n");
        assert_eq!(body, "Body.");
    }

    #[test]
    fn dashes_mid_document_are_not_mistaken_for_frontmatter() {
        let text = "Some text\n---\nnot frontmatter\n---\nmore";
        let (fm, body) = split_frontmatter(text);
        assert_eq!(
            fm, "",
            "frontmatter must start at the very beginning of the file"
        );
        assert_eq!(body, text);
    }
}
