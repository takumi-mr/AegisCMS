pub mod openapi;
pub mod typescript;

pub use openapi::OpenApiGenerator;
pub use typescript::TypeScriptGenerator;

/// Convert kebab-case or snake_case to PascalCase (e.g. "article" -> "Article", "blog_post" -> "BlogPost")
pub fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for c in s.chars() {
        if c == '_' || c == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push_str(&c.to_uppercase().to_string());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    if result.is_empty() {
        "Item".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("article"), "Article");
        assert_eq!(to_pascal_case("blog_post"), "BlogPost");
        assert_eq!(to_pascal_case("tech-category"), "TechCategory");
    }
}
