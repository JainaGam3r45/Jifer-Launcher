//! Build-time configuration that must never be committed.

const CLIENT_ID_VAR: &str = "MICROSOFT_CLIENT_ID";

/// The Azure app (public client) used for Microsoft sign-in. Release builds bake it in at compile time
/// from CI; development builds may also read it from the environment or a local `.env` file.
pub fn microsoft_client_id() -> Option<String> {
    let baked = option_env!("MICROSOFT_CLIENT_ID").map(str::to_owned);
    baked
        .or_else(runtime_client_id)
        .map(|id| id.trim().to_owned())
        .filter(|id| !id.is_empty())
}

#[cfg(debug_assertions)]
fn runtime_client_id() -> Option<String> {
    std::env::var(CLIENT_ID_VAR).ok().or_else(|| {
        let env_file = std::fs::read_to_string(".env")
            .or_else(|_| std::fs::read_to_string("../../../.env"))
            .ok()?;
        dotenv_value(&env_file, CLIENT_ID_VAR)
    })
}

#[cfg(not(debug_assertions))]
fn runtime_client_id() -> Option<String> {
    let _ = CLIENT_ID_VAR;
    None
}

/// Reads `KEY=value` (optionally quoted) from a dotenv file body.
#[cfg_attr(not(debug_assertions), allow(dead_code))]
fn dotenv_value(body: &str, key: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().trim_matches(['"', '\'']).to_owned())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dotenv_lines() {
        let body = "# comment\nJAVA_HOME=C:\\java\nMICROSOFT_CLIENT_ID = \"abc-123\"\n";
        assert_eq!(dotenv_value(body, "MICROSOFT_CLIENT_ID").as_deref(), Some("abc-123"));
        assert_eq!(dotenv_value(body, "MISSING"), None);
    }
}
