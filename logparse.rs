use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct ParsedLine {
    pub ip: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub status_code: Option<i32>,
    pub user_agent: Option<String>,
}

// Formato "combined" padrão do nginx/apache:
// 1.2.3.4 - - [27/Jul/2026:12:00:00 +0000] "GET /path HTTP/1.1" 200 1234 "-" "Mozilla/5.0 ..."
fn combined_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"^(?P<ip>\S+)\s+\S+\s+\S+\s+\[[^\]]+\]\s+"(?P<method>[A-Z]+)\s+(?P<path>\S+)\s+[^"]*"\s+(?P<status>\d{3})\s+\S+\s+"[^"]*"\s+"(?P<ua>[^"]*)""#,
        )
        .expect("regex de log inválida")
    })
}

pub fn parse_line(line: &str) -> Option<ParsedLine> {
    let caps = combined_re().captures(line)?;
    Some(ParsedLine {
        ip: caps.name("ip")?.as_str().to_string(),
        method: caps.name("method").map(|m| m.as_str().to_string()),
        path: caps.name("path").map(|m| m.as_str().to_string()),
        status_code: caps
            .name("status")
            .and_then(|m| m.as_str().parse::<i32>().ok()),
        user_agent: caps.name("ua").map(|m| m.as_str().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_combined_log() {
        let line = r#"203.0.113.5 - - [27/Jul/2026:12:00:00 +0000] "GET /index.html HTTP/1.1" 200 512 "-" "Mozilla/5.0""#;
        let p = parse_line(line).unwrap();
        assert_eq!(p.ip, "203.0.113.5");
        assert_eq!(p.method.as_deref(), Some("GET"));
        assert_eq!(p.status_code, Some(200));
    }
}
