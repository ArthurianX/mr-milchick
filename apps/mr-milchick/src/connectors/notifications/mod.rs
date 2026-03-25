pub mod slack_app;
pub mod slack_workflow;

fn simplify_slack_formatting(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let chars = text.chars().collect::<Vec<_>>();
    let mut index = 0;

    while index < chars.len() {
        match chars[index] {
            '<' => {
                let mut end = index + 1;
                while end < chars.len() && chars[end] != '>' {
                    end += 1;
                }

                if end < chars.len() {
                    let inner = chars[index + 1..end].iter().collect::<String>();
                    if let Some((url, label)) = inner.split_once('|') {
                        output.push_str(label);
                        output.push_str(" (");
                        output.push_str(url);
                        output.push(')');
                    } else {
                        output.push_str(&inner);
                    }
                    index = end + 1;
                    continue;
                }

                output.push('<');
                index += 1;
            }
            '*' | '_' => {
                index += 1;
            }
            ch => {
                output.push(ch);
                index += 1;
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::simplify_slack_formatting;

    #[test]
    fn simplifies_slack_markup_for_plain_renderers() {
        let text = "*Title*\nReview requested for: <https://example.test|MR #1>\n_Assign reviewers_ *@alice*";

        let simplified = simplify_slack_formatting(text);

        assert!(simplified.contains("Title"));
        assert!(simplified.contains("MR #1 (https://example.test)"));
        assert!(simplified.contains("Assign reviewers @alice"));
        assert!(!simplified.contains('<'));
        assert!(!simplified.contains('*'));
        assert!(!simplified.contains('_'));
    }
}
