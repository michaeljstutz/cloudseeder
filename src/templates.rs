use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
};
use std::path::{Path as FsPath, PathBuf};

pub const FILES: [&str; 3] = ["kickstart", "user-data", "meta-data"];

pub fn is_valid_template_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub async fn template_index(
    State(templates_dir): State<PathBuf>,
    Path(template): Path<String>,
) -> Response {
    if resolve_template_dir(&templates_dir, &template)
        .await
        .is_none()
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    Html(render_index(&template)).into_response()
}

pub async fn serve_kickstart(
    State(templates_dir): State<PathBuf>,
    Path(template): Path<String>,
) -> Response {
    serve_file(&templates_dir, &template, "kickstart").await
}

pub async fn serve_user_data(
    State(templates_dir): State<PathBuf>,
    Path(template): Path<String>,
) -> Response {
    serve_file(&templates_dir, &template, "user-data").await
}

pub async fn serve_meta_data(
    State(templates_dir): State<PathBuf>,
    Path(template): Path<String>,
) -> Response {
    serve_file(&templates_dir, &template, "meta-data").await
}

struct Resolved {
    canonical_root: PathBuf,
    canonical_dir: PathBuf,
}

// Resolve and validate a template subdirectory. Returns None (caller should 404) if:
// the name fails validation, the path isn't a directory, canonicalization fails, or
// the resolved directory escapes the configured templates_dir (symlink attack).
async fn resolve_template_dir(templates_dir: &FsPath, template: &str) -> Option<Resolved> {
    if !is_valid_template_name(template) {
        return None;
    }
    let dir = templates_dir.join(template);
    if !dir.is_dir() {
        return None;
    }
    let canonical_root = tokio::fs::canonicalize(templates_dir).await.ok()?;
    let canonical_dir = tokio::fs::canonicalize(&dir).await.ok()?;
    if !canonical_dir.starts_with(&canonical_root) {
        tracing::warn!(
            root = ?canonical_root,
            dir = ?canonical_dir,
            template,
            "template directory escapes templates_dir; rejecting"
        );
        return None;
    }
    Some(Resolved {
        canonical_root,
        canonical_dir,
    })
}

async fn serve_file(templates_dir: &FsPath, template: &str, filename: &str) -> Response {
    let Some(resolved) = resolve_template_dir(templates_dir, template).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let file_path = resolved.canonical_dir.join(filename);
    let body = match tokio::fs::canonicalize(&file_path).await {
        Ok(canonical_file) => {
            if !canonical_file.starts_with(&resolved.canonical_root) {
                tracing::warn!(
                    root = ?resolved.canonical_root,
                    file = ?canonical_file,
                    template,
                    filename,
                    "template file escapes templates_dir; treating as missing"
                );
                Vec::new()
            } else {
                match tokio::fs::read(&canonical_file).await {
                    Ok(bytes) => bytes,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                    Err(e) => {
                        tracing::error!(error = %e, path = ?canonical_file, "template read failed");
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    }
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => {
            tracing::error!(error = %e, path = ?file_path, "template canonicalize failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

// The template name is already validated to [a-z0-9-]+, so no HTML escaping is required.
fn render_index(template: &str) -> String {
    format!(
        "<!doctype html>\n\
<html lang=\"en\"><head><meta charset=\"utf-8\"><title>{template}</title></head>\n\
<body>\n\
<h1>{template}</h1>\n\
<ul>\n\
<li><a href=\"kickstart\">kickstart</a></li>\n\
<li><a href=\"user-data\">user-data</a></li>\n\
<li><a href=\"meta-data\">meta-data</a></li>\n\
</ul>\n\
</body></html>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lowercase_alnum_and_hyphen() {
        assert!(is_valid_template_name("ubuntu-24-04"));
        assert!(is_valid_template_name("rhel9"));
        assert!(is_valid_template_name("a"));
    }

    #[test]
    fn rejects_empty_and_unsafe_names() {
        assert!(!is_valid_template_name(""));
        assert!(!is_valid_template_name("."));
        assert!(!is_valid_template_name(".."));
        assert!(!is_valid_template_name("foo/bar"));
        assert!(!is_valid_template_name("Foo"));
        assert!(!is_valid_template_name("foo_bar"));
        assert!(!is_valid_template_name("foo bar"));
    }
}
