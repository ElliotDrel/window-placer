//! Placement configuration, parsed once at startup with the standard library.
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacementRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug)]
pub struct Rule {
    pub exe: String,
    pub title_contains: Option<String>,
    pub rect: PlacementRect,
}

pub fn load(path: &Path) -> Result<Vec<Rule>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() > 65536 {
        return Err("configuration exceeds 64 KiB".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "configuration must be UTF-8")?;
    parse(text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn parse(text: &str) -> Result<Vec<Rule>, String> {
    let mut rules: Vec<Rule> = Vec::new();
    for (index, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fail = |why: &str| format!("line {}: {why}", index + 1);
        let fields: Vec<_> = line.splitn(6, '|').map(str::trim).collect();
        if fields.len() < 5 {
            return Err(fail(
                "expected exe | x | y | width | height | optional title",
            ));
        }
        let exe = fields[0];
        if !exe.to_ascii_lowercase().ends_with(".exe")
            || exe.contains(['/', '\\', ':'])
            || exe.len() <= 4
        {
            return Err(fail(
                "use an executable file name such as Code.exe, not a path",
            ));
        }
        if rules.iter().any(|r| r.exe.eq_ignore_ascii_case(exe)) {
            return Err(fail("duplicate executable name"));
        }
        if rules.len() >= 256 {
            return Err(fail("at most 256 app rules are supported"));
        }
        let number = |i: usize| {
            fields[i]
                .parse::<i32>()
                .map_err(|_| fail("coordinates and sizes must be whole numbers"))
        };
        let rect = PlacementRect {
            x: number(1)?,
            y: number(2)?,
            width: number(3)?,
            height: number(4)?,
        };
        if rect.width <= 0 || rect.height <= 0 {
            return Err(fail("width and height must be positive"));
        }
        let title_contains = fields
            .get(5)
            .filter(|s| !s.is_empty())
            .map(|s| (*s).to_owned());
        rules.push(Rule {
            exe: exe.to_owned(),
            title_contains,
            rect,
        });
    }
    if rules.is_empty() {
        return Err("configuration contains no app rules".into());
    }
    Ok(rules)
}

pub fn rule_for_exe(rules: &[Rule], exe: &str) -> Option<usize> {
    rules.iter().position(|r| r.exe.eq_ignore_ascii_case(exe))
}

pub fn title_matches(rule: &Rule, title: &str) -> bool {
    rule.title_contains
        .as_deref()
        .is_none_or(|s| title.to_ascii_lowercase().contains(&s.to_ascii_lowercase()))
}

pub fn exe_file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_optional_titles_and_matches_case_insensitively() {
        let rules = parse("# comment\n Code.exe | -10 | 20 | 800 | 600\nWispr Flow.exe | 0 | 0 | 1000 | 700 | Hub | Wispr").unwrap();
        assert_eq!(rule_for_exe(&rules, "CODE.EXE"), Some(0));
        assert!(title_matches(&rules[0], "anything"));
        assert!(title_matches(&rules[1], "hub | wispr flow"));
        assert!(!title_matches(&rules[1], "status"));
        assert_eq!(rules[0].rect.x, -10);
    }
    #[test]
    fn rejects_invalid_configuration() {
        for text in [
            "",
            "# no rules",
            "Code.exe | 0 | 0 | 0 | 500",
            "Code.exe | no | 0 | 400 | 500",
            "C:\\Code.exe | 0 | 0 | 400 | 500",
            "Code.exe | 0 | 0",
            "foo | 0 | 0 | 400 | 500",
            "Code.exe | 0 | 0 | 400 | 500\nCODE.EXE | 0 | 0 | 400 | 500",
        ] {
            assert!(parse(text).is_err(), "accepted {text}");
        }
    }
    #[test]
    fn accepts_utf8_bom_and_empty_optional_filter() {
        let rules = parse("\u{feff}Code.exe | 0 | 0 | 800 | 600 | ").unwrap();
        assert!(rules[0].title_contains.is_none());
    }
    #[test]
    fn shipped_configuration_is_valid() {
        assert_eq!(parse(include_str!("../windows.conf")).unwrap().len(), 11);
    }
    #[test]
    fn file_name_from_path() {
        assert_eq!(exe_file_name(r"C:\Program Files\Code.exe"), "Code.exe");
    }
}
