use std::fmt;
use std::fmt::Write;

use anyhow::Context;
use anyhow::Result;
use colored::Colorize;

use crate::SEPARATOR;
use crate::UnigraphError;

impl fmt::Display for UnigraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.display)
    }
}

impl fmt::Debug for UnigraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatter = ErrorFormatter { colors: true };
        let fmt_s = formatter.format_unigraph_error(self);
        write!(f, "{fmt_s}")?;
        Ok(())
    }
}

/// Take an anyhow error, format it and create a UnigraphError instance.
/// Node: provided error can have UnigraphError as it's root cause. In this case
/// we will end up recreating this UnigraphError with the same `display` value,
/// but also attaching the anyhow chain to its original `debug` value.
pub fn into_unigraph_error(err: &anyhow::Error) -> UnigraphError {
    let formatter = ErrorFormatter { colors: false };
    let s = formatter.format_for_user(err);
    let split = s.split_once(SEPARATOR);

    if let Some((display, debug)) = split {
        UnigraphError {
            display: display.to_string(),
            debug: Some(debug.to_string()),
        }
    } else {
        UnigraphError {
            display: s,
            debug: None,
        }
    }
}

/// Formatting an anyhow error for the end (human) user.
///
/// anyhow already provides a great `Debug` trait that will print the error
/// and all errors in the chain by default.
/// Unfortunately it prints it in the order from most recent `context` to
/// root cause.
/// This has never worked well in any of our products/CLIs, because generally
/// we present errors either in web views, logs or scuba entries. If we format
/// it starting from the most recent `context` we will always end up seeing
/// something very generic, like "thrift request failed" and it is very
/// unhelpful. Especially in situations when someone takes a screenshot of the
/// failure or it is an automatic screenshot from an e2e test.
/// We always end up reproducing the error to see the actual root cause or, even
/// worse, sometimes we just lose the error and the context.
///
/// Formatting also adds visual separations between errors/contexts and terminal
/// colors (parsing the error content in the white wall of terminal output or
/// sandcastle log is extremely hard)
pub fn format_for_user(err: &anyhow::Error) -> String {
    let formatter = ErrorFormatter { colors: false };
    formatter.format_for_user(err)
}

/// See `format_for_user`.
/// Same logic, but with CLI colors.
pub fn format_with_colors(err: &anyhow::Error) -> String {
    let formatter = ErrorFormatter { colors: true };
    formatter.format_for_user(err)
}

/// In some cases we want to force-strip any ANSI colors that are present in the error.
/// Unfortunately `Colorized` only has global overrides which can create some
/// race conditions in tests and this is necessary.
/// Or when we log errors to both STDOUT and Scuba and don't want colors in scuba.
pub fn format_strip_ansi(err: &anyhow::Error) -> String {
    let formatted = format_for_user(err);
    let plain_bytes =
        strip_ansi_escapes::strip(formatted.as_bytes()).expect("failed to strip ansi");
    let stripped = String::from_utf8_lossy(&plain_bytes);
    stripped.into_owned()
}

/// Similar to `format_for_user`, but it also packs it into a JSON containing
/// `UnigraphError` so it can be passed to other external callers (thrift
/// clients or PHPs scripts)
pub fn to_json(err: &anyhow::Error) -> Result<String> {
    let unigraph_error = into_unigraph_error(err);
    serde_json::to_string_pretty(&unigraph_error)
        .context("Failed to serialize UnigraphError to json")
}

struct ErrorFormatter {
    colors: bool,
}

impl ErrorFormatter {
    pub(crate) fn format_unigraph_error(&self, err: &UnigraphError) -> String {
        let mut result = String::new();
        let error_display = self.bold_red(err.display.to_string().trim());
        writeln!(result, "{error_display}").unwrap();
        writeln!(result, "{}", self.dim(SEPARATOR)).unwrap();

        let dbg_str = if let Some(debug) = &err.debug {
            debug.trim().to_string()
        } else {
            "<UnigraphError did not provide any debug info>".to_string()
        };
        let error_debug = self.dim(&dbg_str);
        writeln!(result, "{error_debug}").unwrap();

        result
    }

    pub fn format_for_user(&self, err: &anyhow::Error) -> String {
        let mut chain = vec![];
        let mut result = String::new();
        for next in err.chain() {
            chain.push(next);
        }

        if let Some(root_cause) = chain.pop() {
            if let Some(unigraph_error) = root_cause.downcast_ref::<UnigraphError>() {
                let dbg_fmt = self.format_unigraph_error(unigraph_error);
                writeln!(result, "{dbg_fmt}").unwrap();
            } else {
                let s = self.bold_red(&format!("{root_cause}"));
                writeln!(result, "{s}").unwrap();
            }
        }

        if !chain.is_empty() {
            writeln!(result, "{}", self.dim(SEPARATOR)).unwrap();
            writeln!(result, "{}", self.dim("Error chain and added context:\n")).unwrap();
        }
        for (idx, next_err) in chain.into_iter().rev().enumerate() {
            let next_err_str = self.red(&next_err.to_string());
            let idx_str = self.dim(&format!("[{idx}]:"));
            writeln!(result, "{} {}", idx_str, next_err_str.trim()).unwrap();
            writeln!(result, "{}", self.dim(SEPARATOR)).unwrap();
        }
        result
    }

    fn dim(&self, s: &str) -> String {
        if self.colors {
            s.dimmed().to_string()
        } else {
            s.to_string()
        }
    }

    fn red(&self, s: &str) -> String {
        if self.colors {
            s.red().to_string()
        } else {
            s.to_string()
        }
    }
    fn bold_red(&self, s: &str) -> String {
        if self.colors {
            s.bold().red().to_string()
        } else {
            s.to_string()
        }
    }
}
