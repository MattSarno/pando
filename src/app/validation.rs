pub fn validate_key(key: &str) -> Result<(), String> {
    let slices = key.split('-');
    for slice in slices {
        if slice.is_empty() {
            return Err(
                "key must not be empty and must not have leading, trailing, or double hyphens"
                    .to_string(),
            );
        }

        for character in slice.chars() {
            if !character.is_ascii_lowercase() && !character.is_ascii_digit() {
                return Err(format!(
                    "key segment '{slice}' is invalid — only lowercase letters and digits are allowed"
                ));
            }
        }
    }

    Ok(())
}

pub fn escape_like_wildcards(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
