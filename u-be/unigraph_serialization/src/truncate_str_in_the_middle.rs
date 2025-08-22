// Copyright (c) Meta Platforms, Inc. and affiliates.

pub fn truncate_str_in_the_middle(s: &str, max_bytes: usize) -> String {
    let len = s.len();
    if len > max_bytes {
        let start = &s[..max_bytes / 2];
        let end = &s[(len - max_bytes / 2)..];
        format!(
            "{start}
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

...String is too long. Truncated `{len}` bytes in the middle...

vvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvv
{end}"
        )
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use k9::*;

    use super::*;

    #[test]
    fn test_truncating() {
        let s1 = "1234567890123456789012345678901234567890";
        snapshot!(s1.len(), "40");

        snapshot!(
            truncate_str_in_the_middle(s1, 20),
            "
1234567890
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

...String is too long. Truncated `40` bytes in the middle...

vvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvv
1234567890
"
        );
        snapshot!(
            truncate_str_in_the_middle(s1, 200),
            "1234567890123456789012345678901234567890"
        );
    }
}
