// Copyright (c) Meta Platforms, Inc. and affiliates.

pub enum DocFormat {
    /// /* docs */
    Block,
    /// // docs
    TwoSlash,
}

pub fn render_docs(docs: &Option<String>, format: DocFormat, indent: usize) -> String {
    if let Some(docs) = docs {
        let indent_str = " ".repeat(indent);

        match format {
            DocFormat::Block => {
                let is_multiline = docs.contains('\n');
                if !is_multiline {
                    format!("{indent_str}/** {} */\n", docs.trim())
                } else {
                    let result: String = docs
                        .lines()
                        .map(|line| format!("{} * {}\n", indent_str, line))
                        .collect();
                    format!("{indent_str}/**\n{}\n{indent_str} */\n", result.trim_end())
                }
            }
            DocFormat::TwoSlash => docs
                .lines()
                .map(|line| format!("{}// {}\n", indent_str, line))
                .collect::<String>(),
        }
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    #[test]
    fn test_render_docs() {
        let missing = render_docs(&None, DocFormat::Block, 2);
        snapshot!(missing, "");

        let single_line_ss = render_docs(&Some("meow".to_string()), DocFormat::TwoSlash, 2);
        snapshot!(
            single_line_ss,
            "
  // meow

"
        );
        let multiline_line_ss = render_docs(
            &Some("meow\nwoof\n\napple".to_string()),
            DocFormat::TwoSlash,
            4,
        );
        snapshot!(
            multiline_line_ss,
            "
    // meow
    // woof
    // 
    // apple

"
        );

        let single_line_block = render_docs(&Some("meow".to_string()), DocFormat::Block, 2);
        snapshot!(
            single_line_block,
            "
  /** meow */

"
        );
        let multiline_line_block = render_docs(
            &Some("meow\nwoof\n\napple".to_string()),
            DocFormat::Block,
            4,
        );
        snapshot!(
            multiline_line_block,
            "
    /**
     * meow
     * woof
     * 
     * apple
     */

"
        );
    }
}
