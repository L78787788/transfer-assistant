use std::{collections::HashSet, fs, path::Path, time::UNIX_EPOCH};

/// 为目标路径分配不重名的名称：优先原名，冲突时按 `名称 (n)` 递增。
pub(crate) fn unique_target_name(
    directory: &Path,
    requested: &str,
    reserved: &mut HashSet<String>,
) -> String {
    for index in 0_u32.. {
        let candidate = if index == 0 {
            requested.to_owned()
        } else {
            suffixed_name(requested, index)
        };
        let key = if cfg!(windows) {
            candidate.to_lowercase()
        } else {
            candidate.clone()
        };
        if !directory.join(&candidate).exists() && reserved.insert(key) {
            return candidate;
        }
    }
    unreachable!("u32 name suffix space exhausted")
}

pub(crate) fn suffixed_name(name: &str, index: u32) -> String {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(name);
    match path.extension().and_then(|value| value.to_str()) {
        Some(extension) if !extension.is_empty() => format!("{stem} ({index}).{extension}"),
        _ => format!("{name} ({index})"),
    }
}

pub(crate) fn sanitize_target_segment(segment: &str) -> String {
    // Reject directory traversal on all platforms.
    if segment == "." || segment == ".." {
        return "_".to_owned();
    }
    if !cfg!(windows) {
        return segment.to_owned();
    }
    let mut sanitized = segment
        .chars()
        .map(|character| {
            if character < ' '
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    sanitized = sanitized.trim_end_matches([' ', '.']).to_owned();
    if sanitized.is_empty() {
        sanitized.push('_');
    }
    let stem = sanitized
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');
    if reserved {
        sanitized.push('_');
    }
    sanitized
}

pub(crate) fn safe_manifest_name(name: &str) -> String {
    let cleaned = name
        .chars()
        .map(|character| {
            if matches!(character, '/' | '\\' | '\0') {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "未命名项目".to_owned()
    } else {
        cleaned.to_owned()
    }
}

pub(crate) fn unique_manifest_root(mut name: String, seen: &mut HashSet<String>) -> String {
    let original = name.clone();
    let mut index = 1_u32;
    while !seen.insert(name.to_lowercase()) {
        name = suffixed_name(&original, index);
        index += 1;
    }
    name
}

pub(crate) fn source_revision(metadata: &fs::Metadata) -> String {
    format!("{}:{}", metadata.len(), modified_unix_ms(metadata))
}

pub(crate) fn modified_unix_ms(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(windows)]
pub(crate) fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(crate) fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}
