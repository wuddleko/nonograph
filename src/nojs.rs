pub fn strip_javascript(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' {
            let mut tag_chars = Vec::new();
            tag_chars.push(ch);

            let mut temp_chars = chars.clone();
            let mut is_script_tag = false;
            let mut is_end_tag = false;

            if let Some('/') = temp_chars.peek() {
                tag_chars.push(temp_chars.next().unwrap());
                is_end_tag = true;
            }

            let mut tag_name = String::new();
            while let Some(&next_ch) = temp_chars.peek() {
                if next_ch.is_whitespace() || next_ch == '>' || next_ch == '/' {
                    break;
                }
                tag_name.push(temp_chars.next().unwrap());
                tag_chars.push(tag_name.chars().last().unwrap());
            }

            if tag_name.to_lowercase() == "script" {
                is_script_tag = true;
            }

            if is_script_tag {
                if is_end_tag {
                    while let Some(ch) = chars.next() {
                        if ch == '>' {
                            break;
                        }
                    }
                } else {
                    while let Some(ch) = chars.next() {
                        if ch == '>' {
                            break;
                        }
                    }

                    let mut in_script = true;
                    while in_script && chars.peek().is_some() {
                        if let Some('<') = chars.peek() {
                            let mut temp_chars = chars.clone();
                            temp_chars.next();

                            if let Some('/') = temp_chars.peek() {
                                temp_chars.next();

                                let mut closing_tag = String::new();
                                while let Some(&next_ch) = temp_chars.peek() {
                                    if next_ch.is_whitespace() || next_ch == '>' {
                                        break;
                                    }
                                    closing_tag.push(temp_chars.next().unwrap());
                                }

                                if closing_tag.to_lowercase() == "script" {
                                    chars.next();
                                    chars.next();
                                    for _ in 0..6 {
                                        chars.next();
                                    }
                                    while let Some(ch) = chars.next() {
                                        if ch == '>' {
                                            break;
                                        }
                                    }
                                    in_script = false;
                                }
                            }
                        }

                        if in_script {
                            chars.next();
                        }
                    }
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
#[path = "../test/nojs.rs"]
mod tests;
