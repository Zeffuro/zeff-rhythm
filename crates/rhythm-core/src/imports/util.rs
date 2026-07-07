use super::ImportError;

pub(super) fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(':')?;
    Some((key.trim(), value.trim()))
}

pub(super) fn split_csv(line: &str) -> Vec<&str> {
    line.split(',').map(str::trim).collect()
}

pub(super) fn strip_bom(line: &str) -> &str {
    line.trim_start_matches('\u{feff}')
}

pub(super) fn strip_line_comment(line: &str) -> &str {
    line.split_once("//")
        .map(|(value, _)| value)
        .unwrap_or(line)
        .trim()
}

pub(super) fn parse_u8(value: &str, line: usize, name: &str) -> Result<u8, ImportError> {
    value
        .parse()
        .map_err(|_| ImportError::at_line(line, format!("invalid {name}")))
}

pub(super) fn parse_i32(value: &str, line: usize, name: &str) -> Result<i32, ImportError> {
    value
        .parse()
        .map_err(|_| ImportError::at_line(line, format!("invalid {name}")))
}

pub(super) fn parse_f64(value: &str, line: usize, name: &str) -> Result<f64, ImportError> {
    value
        .parse()
        .map_err(|_| ImportError::at_line(line, format!("invalid {name}")))
}
