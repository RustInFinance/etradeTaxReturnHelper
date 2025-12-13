// SPDX-FileCopyrightText: 2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

//! Path utility module for anonymizer.
//!
//! Provides helper functions for generating anonymized output file paths
//! by prefixing filenames with `anonymous_` while preserving directory structure.

use std::path::PathBuf;

/// Build an output path by prefixing the input filename with `anonymous_`.
///
/// Preserves the parent directory if present and returns a `PathBuf`.
///
/// # Examples
/// ```ignore
/// use std::path::Path;
/// let input = Path::new("data/statement.pdf");
/// let output = anonymous_output_path(input);
/// assert_eq!(output, Path::new("data/anonymous_statement.pdf"));
/// ```
pub(crate) fn anonymous_output_path(input_path: &std::path::Path) -> PathBuf {
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
        let in_path = std::path::Path::new("statement.pdf");
        let out = anonymous_output_path(in_path);
        assert_eq!(out, std::path::PathBuf::from("anonymous_statement.pdf"));
    }

    #[test]
    fn test_anonymous_output_path_with_parent() {
        let in_path = std::path::Path::new("some/dir/statement.pdf");
        let out = anonymous_output_path(in_path);
        assert_eq!(
            out,
            std::path::PathBuf::from("some/dir/anonymous_statement.pdf")
        );
    }

    #[test]
    fn test_anonymous_output_path_unicode_filename() {
        let in_path = std::path::Path::new("résumé.pdf");
        let out = anonymous_output_path(in_path);
        assert_eq!(out, std::path::PathBuf::from("anonymous_résumé.pdf"));
    }
}
