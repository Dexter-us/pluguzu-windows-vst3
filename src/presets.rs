// Pluguzu presets

use smol_str::SmolStr;
use std::io::Write;
use std::sync::Arc;

use crate::debug::{info, pwarn};
use crate::haskell::UzuKind;

#[derive(Debug, PartialEq)]
pub struct UzuProgram {
    pub name: String,
    pub lang: UzuKind,
    pub code: String,
    pub tags: Vec<SmolStr>,
}

pub type Presets = Vec<Arc<UzuProgram>>;
pub type PresetsTags = Vec<(String, Presets)>;

pub(crate) fn load_default_presets() -> PresetsTags {
    let presets = crate::default_presets::presets();
    let mut map = indexmap::IndexMap::with_capacity(presets.len());
    for preset in presets {
        let preset = Arc::new(preset);
        for tag in &preset.tags {
            map.entry(tag.to_string())
                .or_insert_with(Vec::new)
                .push(preset.clone())
        }
    }
    map.into_iter().collect()
}

pub(crate) fn load_user_presets(path: &std::path::PathBuf) -> Presets {
    if let Ok(preset_data) = std::fs::read_to_string(path) {
        parse_presets(&preset_data)
    } else {
        vec![]
    }
}

pub(crate) fn add_preset(path: &std::path::PathBuf, lang: UzuKind, code: &str) -> Arc<UzuProgram> {
    let now = time::UtcDateTime::now();
    let format = time::format_description::parse("[year]-[month]-[day]-[hour]:[minute]:[second]")
        .expect("format");
    let name = now.format(&format).unwrap_or("N/A".to_string());
    let lang_str = match lang {
        UzuKind::Tidal => "haskell",
        UzuKind::Mondo => "mondo",
    };
    match std::fs::File::options()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(mut f) => {
            let _ = writeln!(&mut f, "\n## {name}\n\n```{lang_str}\n{code}\n```");
            info!("{}: Updated presets", path.display());
        }
        Err(err) => {
            pwarn!("{}: Couldn't save preset: {}", path.display(), err);
        }
    }
    Arc::new(UzuProgram {
        name,
        lang,
        code: code.to_string(),
        tags: vec![],
    })
}

fn parse_presets(data: &str) -> Presets {
    let mut presets = vec![];
    let mut name = None;
    let mut lines = data.lines();
    while let Some(line) = lines.next() {
        if let Some(pname) = line.strip_prefix("## ") {
            name = Some(pname.to_string());
        }
        let lang = if line == "```haskell" {
            Some(UzuKind::Tidal)
        } else if line == "```mondo" {
            Some(UzuKind::Mondo)
        } else {
            None
        };
        if let Some(lang) = lang
            && let Some(name) = name.take()
        {
            let mut code = vec![];
            for line in lines.by_ref() {
                if line == "```" {
                    break;
                }
                code.push(line);
            }
            presets.push(Arc::new(UzuProgram {
                name,
                lang,
                code: code.join("\n").to_string(),
                tags: vec![],
            }))
        }
    }
    presets
}

#[test]
fn test_parse_presets() {
    let presets = parse_presets(
        r#"
## test

```haskell
s "bd*4"
```
"#,
    );
    assert_eq!(
        presets,
        vec![Arc::new(UzuProgram {
            name: "test".to_string(),
            lang: crate::UzuKind::Tidal,
            code: "s \"bd*4\"".to_string(),
            tags: vec![]
        })]
    );
}
