//! Fail-closed verification of the clone-server's schema-scoped flags-2-env
//! manifest. This checks the source contract, not runtime parser integration.
use std::collections::BTreeMap;

fn verify(source: &str) -> Result<(), &'static str> {
    let mut section = "";
    let mut seen = BTreeMap::new();
    for line in source.lines() {
        let value = line.trim();
        if value.is_empty() || value.starts_with('#') {
            continue;
        }
        if value.starts_with('[') {
            section = match value {
                "[identity]" => "identity",
                "[parse]" => "parse",
                "[env]" => "env",
                _ => return Err("unknown or nested manifest section"),
            };
            continue;
        }
        let (key, value) = value
            .split_once('=')
            .ok_or("expected a literal key-value assignment")?;
        let key = key.trim();
        let value = value.trim();
        let expected = match (section, key) {
            ("identity", "service") => "\"gha-clone-server\"",
            ("identity", "type_name") => "\"RuntimeEnv\"",
            ("parse", "allow_unknown") => "false",
            ("env", "load") => "false",
            ("env", "files") => "[]",
            _ => return Err("unexpected manifest key or section"),
        };
        if value != expected {
            return Err("manifest authority widened");
        }
        if seen.insert((section, key), ()).is_some() {
            return Err("duplicate manifest authority");
        }
    }
    if seen.len() != 5 {
        return Err("missing required manifest authority");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::verify;

    const CONTRACT: &str = include_str!("../.cli-flags.toml");

    #[test]
    fn committed_contract_is_closed() {
        assert!(verify(CONTRACT).is_ok());
    }

    #[test]
    fn unknown_options_and_dotenv_are_deny_only() {
        for mutation in [
            CONTRACT.replace("allow_unknown = false", "allow_unknown = true"),
            CONTRACT.replace("load = false", "load = true"),
            CONTRACT.replace("files = []", "files = [\".env\"]"),
        ] {
            assert!(verify(&mutation).is_err());
        }
    }

    #[test]
    fn duplicate_authority_and_unknown_section_are_rejected() {
        assert!(verify(&format!("{CONTRACT}\n[parse]\nallow_unknown = true\n")).is_err());
        assert!(verify(&format!("{CONTRACT}\n[untrusted]\nallow_unknown = false\n")).is_err());
    }

    #[test]
    fn source_and_type_identity_are_exact() {
        assert!(verify(&CONTRACT.replace("gha-clone-server", "other-service")).is_err());
        assert!(verify(&CONTRACT.replace("RuntimeEnv", "UnsafeEnv")).is_err());
    }

    #[test]
    fn missing_or_malformed_keys_cannot_pass() {
        assert!(verify(&CONTRACT.replace("files = []", "")).is_err());
        assert!(verify(&CONTRACT.replace("allow_unknown = false", "allow_unknown: false")).is_err());
    }
}

fn main() {
    match verify(include_str!("../.cli-flags.toml")) {
        Ok(()) => println!("CLOSED_FLAGS_CONTRACT_PASS"),
        Err(error) => {
            eprintln!("CLOSED_FLAGS_CONTRACT_FAIL: {error}");
            std::process::exit(1);
        }
    }
}
