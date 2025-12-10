use std::path::PathBuf;

/// Build an output path by prefixing the input filename with `anonymous_`.
///
/// Preserves the parent directory if present and returns a `PathBuf`.
pub(crate) fn anonymous_output_path<P: AsRef<std::path::Path>>(in_path: P) -> PathBuf {
    let input_path = in_path.as_ref();

    let file_name = input_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| input_path.to_string_lossy().into_owned());

    if let Some(parent) = input_path.parent() {
        let mut pb = PathBuf::from(parent);
        pb.push(format!("anonymous_{}", file_name));
        pb
    } else {
        PathBuf::from(format!("anonymous_{}", file_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymous_output_path_no_parent() {
        let in_path = "statement.pdf";
        let out = anonymous_output_path(in_path);
        assert_eq!(out, std::path::PathBuf::from("anonymous_statement.pdf"));
    }

    #[test]
    fn test_anonymous_output_path_with_parent() {
        let in_path = "some/dir/statement.pdf";
        let out = anonymous_output_path(in_path);
        assert_eq!(out, std::path::PathBuf::from("some/dir/anonymous_statement.pdf"));
    }

    #[test]
    fn test_anonymous_output_path_unicode_filename() {
        let in_path = "résumé.pdf";
        let out = anonymous_output_path(in_path);
        assert_eq!(out, std::path::PathBuf::from("anonymous_résumé.pdf"));
    }
}
