// Copyright (C) 2026  Clyso
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Minimal column-aligned table rendering for CLI list output.
//!
//! Column widths are derived from the data, not hard-coded, so a cell wider
//! than its header never shoves the following columns out of alignment.

/// Format a left-aligned table.
///
/// Each column is padded to the widest of its header and its cells, measured in
/// Unicode scalar values (`chars`) to match how `std::fmt`'s `{:<width$}` pads.
/// Rows are indented two spaces and columns are separated by a two-space gutter,
/// matching the existing `cbc` list style. The final column is never padded, so
/// there is no trailing whitespace and a variable-length tail column (e.g. a
/// comma-joined role list) needs no width budget.
///
/// Rows shorter than `headers` are padded with empty cells; cells beyond
/// `headers.len()` are ignored.
pub fn format_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let cols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(cols) {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }

    let mut out = String::new();
    render_row(&mut out, &widths, headers.iter().copied());
    for row in rows {
        render_row(&mut out, &widths, row.iter().map(String::as_str));
    }
    out
}

/// Append one indented, padded row (terminated by a newline) to `out`.
///
/// The row is assembled in a scratch buffer and `trim_end`ed before it lands in
/// `out`, so a row whose trailing columns are empty (e.g. a pending user with no
/// roles) carries no trailing whitespace from the padded interior columns.
fn render_row<'a>(out: &mut String, widths: &[usize], mut cells: impl Iterator<Item = &'a str>) {
    use std::fmt::Write as _;

    let cols = widths.len();
    let mut line = String::from("  "); // two-space indent, existing style
    for (i, &width) in widths.iter().enumerate() {
        let cell = cells.next().unwrap_or("");
        if i + 1 == cols {
            line.push_str(cell); // last column: never padded
        } else {
            // `write!` to a String is infallible; the result is ignored
            // deliberately rather than unwrapped.
            let _ = write!(line, "{cell:<width$}  ");
        }
    }
    out.push_str(line.trim_end());
    out.push('\n');
}

/// Print a [`format_table`] result to stdout.
pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    print!("{}", format_table(headers, rows));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn columns_align_when_cell_exceeds_header() {
        // EMAIL header is 5 chars but the cell is 25; NAME header is 4 but the
        // cell is 19. Every column must start at the same offset on both rows.
        let headers = ["EMAIL", "NAME", "ROLES"];
        let rows = vec![
            row(&["bryan.stillwell@clyso.com", "Christian Schupfner", "viewer"]),
            row(&["a@b.co", "Al", "admin, builder"]),
        ];
        let out = format_table(&headers, &rows);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3, "header + two rows");

        // The widest EMAIL is 25 chars; NAME starts after "  " + 25 + "  ".
        // All cells here are ASCII, so byte offset == char offset.
        let name_offset = 2 + 25 + 2;
        assert!(lines[0][name_offset..].starts_with("NAME"));
        assert!(lines[1][name_offset..].starts_with("Christian Schupfner"));
        assert!(lines[2][name_offset..].starts_with("Al"));
    }

    #[test]
    fn last_column_has_no_trailing_whitespace() {
        let headers = ["A", "B"];
        let rows = vec![row(&["x", "y"])];
        let out = format_table(&headers, &rows);
        for line in out.lines() {
            assert_eq!(line, line.trim_end(), "no trailing whitespace: {line:?}");
        }
    }

    #[test]
    fn short_rows_are_padded_with_empty_cells() {
        let headers = ["A", "B", "C"];
        let rows = vec![row(&["x"])];
        let out = format_table(&headers, &rows);
        let lines: Vec<&str> = out.lines().collect();
        // Header line still has all three columns; the short data row does not
        // panic and pads the missing middle column, leaving an empty tail.
        assert!(lines[1].starts_with("  x"));
        assert_eq!(lines[1], lines[1].trim_end());
    }
}
