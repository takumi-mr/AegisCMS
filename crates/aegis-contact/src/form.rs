//! 入力の検査と、フォームの HTML。`/contact/{name}` のページと本文のブロック（`core/contact-form`）で
//! 同じ HTML を使う（テーマの `contact.html` は `fields.form` から自分で組み立ててもよい）

use std::collections::HashMap;

use aegis_ast::renderer::escape_html;
use serde::Serialize;

use crate::config::{FieldConfig, FieldKind, FormConfig, HONEYPOT_FIELD};

/// 入力の誤り（項目ごと）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub field: String,
    /// `required` / `too_long` / `invalid_email` / `invalid_tel` / `invalid_option` / `invalid_text`
    pub code: String,
    /// 送信者に見せる文言（英語。テーマで訳すときは `code` を使う）
    pub message: String,
}

/// 検査を通った 1 項目の値（保存・通知・受信箱に出す形）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct FieldValue {
    pub name: String,
    pub label: String,
    /// 入力された値（チェックボックスは `yes` か空、ファイルはファイル名）
    pub value: String,
    /// 添付されたファイル（`type = "file"` の項目）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<Attachment>,
}

/// 保存した添付ファイル。Admin Studio からだけ取り出せる（公開 URL は無い）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Attachment {
    /// ストレージのキー
    pub key: String,
    /// 送信されたファイル名（パス・使えない文字を除いたもの）
    pub filename: String,
    pub size: usize,
    pub content_type: String,
}

/// 送信された添付ファイル（`field` は項目の名前）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadedFile {
    pub field: String,
    pub filename: String,
    pub data: Vec<u8>,
}

/// 添付ファイルの決まり（`[contact].max_attachment_mb` / `attachment_extensions`）
#[derive(Debug, Clone, Copy, Default)]
pub struct FileRules<'a> {
    pub max_bytes: usize,
    pub extensions: &'a [String],
}

/// ファイル名からパスと使えない文字を除く（拡張子は残す）。空になったら `None`
pub fn sanitize_filename(raw: &str) -> Option<String> {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace());
    if cleaned.is_empty() {
        return None;
    }
    let (stem, ext) = match cleaned.rsplit_once('.') {
        Some((stem, ext)) => (stem, format!(".{}", ext)),
        None => (cleaned, String::new()),
    };
    let keep = 100usize.saturating_sub(ext.chars().count()).max(1);
    Some(format!(
        "{}{}",
        stem.chars().take(keep).collect::<String>(),
        ext
    ))
}

/// ファイル名の拡張子（小文字。無ければ空）
pub fn extension_of(filename: &str) -> String {
    filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

fn check_file(
    field: &FieldConfig,
    file: Option<&UploadedFile>,
    rules: &FileRules,
    locale: &str,
) -> Result<String, FieldError> {
    let err = |code: &str, detail: &str| FieldError {
        field: field.name.clone(),
        code: code.to_string(),
        message: crate::messages::file_error(locale, code, &field.label, detail),
    };
    // ファイルを選ばずに送ると、ブラウザは空のファイル名・空の中身で送ってくる
    let Some(file) = file.filter(|f| !f.data.is_empty() || !f.filename.trim().is_empty()) else {
        return if field.required {
            Err(err("required", ""))
        } else {
            Ok(String::new())
        };
    };
    if file.data.is_empty() {
        return Err(err("required", ""));
    }
    if file.data.len() > rules.max_bytes {
        return Err(err(
            "file_too_large",
            &(rules.max_bytes / (1024 * 1024)).to_string(),
        ));
    }
    let allowed = sanitize_filename(&file.filename)
        .filter(|name| rules.extensions.contains(&extension_of(name)));
    allowed.ok_or_else(|| err("invalid_file_type", &rules.extensions.join(", ")))
}

fn error(field: &FieldConfig, code: &str, locale: &str) -> FieldError {
    // チェックボックスの必須は「チェックを入れてください」の文言にする（code は required のまま）
    let message_code = if code == "required" && field.kind == FieldKind::Checkbox {
        "required_checkbox"
    } else {
        code
    };
    FieldError {
        field: field.name.clone(),
        code: code.to_string(),
        message: crate::messages::field_error(
            locale,
            message_code,
            &field.label,
            field.max_chars(),
        ),
    }
}

/// 送られてきた値（項目の名前 → 値）を検査して、フォームの項目の順に並べる。誤りの文言は `locale` の言語
pub fn validate(
    form: &FormConfig,
    values: &HashMap<String, String>,
    locale: &str,
) -> Result<Vec<FieldValue>, Vec<FieldError>> {
    validate_with_files(form, values, &[], &FileRules::default(), locale)
}

/// [`validate`] に添付ファイルを加えたもの。`type = "file"` の項目の値はファイル名（`file` は保存のときに埋める）
pub fn validate_with_files(
    form: &FormConfig,
    values: &HashMap<String, String>,
    files: &[UploadedFile],
    rules: &FileRules,
    locale: &str,
) -> Result<Vec<FieldValue>, Vec<FieldError>> {
    let mut out = Vec::with_capacity(form.fields.len());
    let mut errors = Vec::new();
    for field in &form.fields {
        let checked = if field.kind == FieldKind::File {
            let file = files.iter().find(|f| f.field == field.name);
            check_file(field, file, rules, locale)
        } else {
            let raw = values.get(&field.name).map(String::as_str).unwrap_or("");
            check_field(field, raw, locale)
        };
        match checked {
            Ok(value) => out.push(FieldValue {
                name: field.name.clone(),
                label: field.label.clone(),
                value,
                file: None,
            }),
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

fn check_field(field: &FieldConfig, raw: &str, locale: &str) -> Result<String, FieldError> {
    if field.kind == FieldKind::Checkbox {
        let checked = matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "yes" | "on" | "true" | "1"
        );
        if field.required && !checked {
            return Err(error(field, "required", locale));
        }
        return Ok(if checked { "yes" } else { "" }.to_string());
    }
    let multiline = field.kind == FieldKind::Textarea;
    let value = if multiline {
        raw.replace("\r\n", "\n")
            .replace('\r', "\n")
            .trim()
            .to_string()
    } else {
        raw.trim().to_string()
    };
    if value.is_empty() {
        if field.required {
            return Err(error(field, "required", locale));
        }
        return Ok(String::new());
    }
    if value.chars().count() > field.max_chars() {
        return Err(error(field, "too_long", locale));
    }
    if value
        .chars()
        .any(|c| c.is_control() && !(multiline && (c == '\n' || c == '\t')))
    {
        return Err(error(field, "invalid_text", locale));
    }
    match field.kind {
        FieldKind::Email => aegis_comments::moderation::normalize_email(&value)
            .map_err(|_| error(field, "invalid_email", locale)),
        FieldKind::Tel => {
            let ok = value
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '(' | ')' | ' ' | '.'))
                && value.chars().filter(char::is_ascii_digit).count() >= 3;
            if ok {
                Ok(value)
            } else {
                Err(error(field, "invalid_tel", locale))
            }
        }
        FieldKind::Select => {
            if field.options.iter().any(|o| o == &value) {
                Ok(value)
            } else {
                Err(error(field, "invalid_option", locale))
            }
        }
        _ => Ok(value),
    }
}

/// フォームの HTML（エスケープ済み）。`values` は再表示する入力、`errors` は項目の下に出す誤り
pub fn form_html(
    form: &FormConfig,
    action: &str,
    values: &HashMap<String, String>,
    errors: &[FieldError],
) -> String {
    form_html_with(form, action, values, errors, &[])
}

/// [`form_html`] に、添付できる拡張子（`type = "file"` の項目の `accept`）を加えたもの
pub fn form_html_with(
    form: &FormConfig,
    action: &str,
    values: &HashMap<String, String>,
    errors: &[FieldError],
    extensions: &[String],
) -> String {
    let id = |field: &str| format!("aegis-contact-{}-{}", form.name, field);
    let has_file = form.fields.iter().any(|f| f.kind == FieldKind::File);
    let accept = extensions
        .iter()
        .map(|e| format!(".{}", e))
        .collect::<Vec<_>>()
        .join(",");
    let mut html = format!(
        "<form class=\"aegis-contact-form\" method=\"post\" action=\"{}\"{} accept-charset=\"utf-8\">\n",
        escape_html(action),
        if has_file {
            " enctype=\"multipart/form-data\""
        } else {
            ""
        }
    );
    if !form.description.trim().is_empty() {
        html.push_str(&format!(
            "<p class=\"aegis-contact-description\">{}</p>\n",
            escape_html(form.description.trim()).replace('\n', "<br>")
        ));
    }
    // ボット除け: 人には見えない欄。値が入っていたら保存しない
    html.push_str(&format!(
        "<div class=\"aegis-contact-trap\" aria-hidden=\"true\" style=\"position:absolute;left:-10000px;width:1px;height:1px;overflow:hidden\">\
         <label>Leave this empty <input type=\"text\" name=\"{}\" tabindex=\"-1\" autocomplete=\"off\"></label></div>\n",
        HONEYPOT_FIELD
    ));
    for field in &form.fields {
        let field_id = id(&field.name);
        let value = values.get(&field.name).map(String::as_str).unwrap_or("");
        let field_error = errors.iter().find(|e| e.field == field.name);
        let error_id = format!("{}-error", field_id);
        let mut attrs = format!(
            " id=\"{}\" name=\"{}\"",
            escape_html(&field_id),
            escape_html(&field.name)
        );
        if field.required {
            attrs.push_str(" required");
        }
        if field_error.is_some() {
            attrs.push_str(&format!(
                " aria-invalid=\"true\" aria-describedby=\"{}\"",
                escape_html(&error_id)
            ));
        }
        let required_mark = if field.required {
            "<span class=\"aegis-contact-required\" aria-hidden=\"true\"> *</span>"
        } else {
            ""
        };
        html.push_str(&format!(
            "<div class=\"aegis-contact-field aegis-contact-{}\">\n",
            match field.kind {
                FieldKind::Text => "text",
                FieldKind::Email => "email",
                FieldKind::Tel => "tel",
                FieldKind::Textarea => "textarea",
                FieldKind::Select => "select",
                FieldKind::Checkbox => "checkbox",
                FieldKind::File => "file",
            }
        ));
        if field.kind == FieldKind::Checkbox {
            let checked = matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "yes" | "on" | "true" | "1"
            );
            html.push_str(&format!(
                "<label><input type=\"checkbox\" value=\"yes\"{}{}> {}{}</label>\n",
                attrs,
                if checked { " checked" } else { "" },
                escape_html(&field.label),
                required_mark
            ));
        } else {
            html.push_str(&format!(
                "<label for=\"{}\">{}{}</label>\n",
                escape_html(&field_id),
                escape_html(&field.label),
                required_mark
            ));
            let placeholder = if field.placeholder.is_empty() {
                String::new()
            } else {
                format!(" placeholder=\"{}\"", escape_html(&field.placeholder))
            };
            let maxlength = format!(" maxlength=\"{}\"", field.max_chars());
            match field.kind {
                FieldKind::Textarea => html.push_str(&format!(
                    "<textarea{}{}{} rows=\"6\">{}</textarea>\n",
                    attrs,
                    placeholder,
                    maxlength,
                    escape_html(value)
                )),
                FieldKind::Select => {
                    html.push_str(&format!("<select{}>\n", attrs));
                    html.push_str(&format!(
                        "<option value=\"\">{}</option>\n",
                        escape_html(if field.placeholder.is_empty() {
                            "—"
                        } else {
                            &field.placeholder
                        })
                    ));
                    for option in &field.options {
                        html.push_str(&format!(
                            "<option{}>{}</option>\n",
                            if option == value { " selected" } else { "" },
                            escape_html(option)
                        ));
                    }
                    html.push_str("</select>\n");
                }
                FieldKind::File => html.push_str(&format!(
                    "<input type=\"file\"{}{}>\n",
                    attrs,
                    if accept.is_empty() {
                        String::new()
                    } else {
                        format!(" accept=\"{}\"", escape_html(&accept))
                    }
                )),
                kind => {
                    let (input_type, autocomplete) = match kind {
                        FieldKind::Email => ("email", " autocomplete=\"email\""),
                        FieldKind::Tel => ("tel", " autocomplete=\"tel\""),
                        _ if field.name == "name" => ("text", " autocomplete=\"name\""),
                        _ => ("text", ""),
                    };
                    html.push_str(&format!(
                        "<input type=\"{}\"{}{}{}{} value=\"{}\">\n",
                        input_type,
                        attrs,
                        autocomplete,
                        placeholder,
                        maxlength,
                        escape_html(value)
                    ));
                }
            }
        }
        if let Some(e) = field_error {
            html.push_str(&format!(
                "<p class=\"aegis-contact-error\" id=\"{}\">{}</p>\n",
                escape_html(&error_id),
                escape_html(&e.message)
            ));
        }
        html.push_str("</div>\n");
    }
    html.push_str(&format!(
        "<button type=\"submit\" class=\"aegis-contact-submit\">{}</button>\n</form>",
        escape_html(&form.submit_label)
    ));
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_fields;

    fn base_form() -> FormConfig {
        FormConfig {
            fields: default_fields("en"),
            ..FormConfig::default()
        }
    }

    fn values(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn form_with(extra: Vec<FieldConfig>) -> FormConfig {
        let mut form = base_form();
        form.fields.extend(extra);
        form
    }

    #[test]
    fn test_validate_orders_and_normalizes() {
        let form = base_form();
        let out = validate(
            &form,
            &values(&[
                ("message", "  こんにちは\r\n2 行目  "),
                ("email", " Hanako@Example.COM "),
                ("name", " 花子 "),
                ("extra", "ignored"),
            ]),
            "en",
        )
        .unwrap();
        let pairs: Vec<(&str, &str)> = out
            .iter()
            .map(|v| (v.name.as_str(), v.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("name", "花子"),
                ("email", "hanako@example.com"),
                ("subject", ""),
                ("message", "こんにちは\n2 行目"),
            ]
        );
    }

    #[test]
    fn test_validate_reports_every_error() {
        let mut tel = FieldConfig {
            name: "tel".into(),
            label: "Phone".into(),
            kind: FieldKind::Tel,
            required: false,
            placeholder: String::new(),
            options: vec![],
            max_length: None,
        };
        let mut plan = tel.clone();
        plan.name = "plan".into();
        plan.kind = FieldKind::Select;
        plan.options = vec!["Free".into(), "Pro".into()];
        let mut agree = tel.clone();
        agree.name = "agree".into();
        agree.kind = FieldKind::Checkbox;
        agree.required = true;
        tel.max_length = Some(20);
        let form = form_with(vec![tel, plan, agree]);
        let errors = validate(
            &form,
            &values(&[
                ("email", "not-an-email"),
                ("message", &"x".repeat(5001)),
                ("tel", "call me"),
                ("plan", "Enterprise"),
            ]),
            "en",
        )
        .unwrap_err();
        let codes: Vec<(&str, &str)> = errors
            .iter()
            .map(|e| (e.field.as_str(), e.code.as_str()))
            .collect();
        assert_eq!(
            codes,
            vec![
                ("name", "required"),
                ("email", "invalid_email"),
                ("message", "too_long"),
                ("tel", "invalid_tel"),
                ("plan", "invalid_option"),
                ("agree", "required"),
            ]
        );
    }

    #[test]
    fn test_checkbox_and_control_characters() {
        let mut agree = default_fields("en")[0].clone();
        agree.name = "agree".into();
        agree.kind = FieldKind::Checkbox;
        agree.required = false;
        let form = form_with(vec![agree]);
        let ok = validate(
            &form,
            &values(&[
                ("name", "A"),
                ("email", "a@example.com"),
                ("message", "tab\tand\nnewline"),
                ("agree", "on"),
            ]),
            "en",
        )
        .unwrap();
        assert_eq!(ok.last().unwrap().value, "yes");
        let bad = validate(
            &form,
            &values(&[
                ("name", "A\u{7}"),
                ("email", "a@example.com"),
                ("message", "x"),
            ]),
            "en",
        )
        .unwrap_err();
        assert_eq!(bad[0].code, "invalid_text");
    }

    #[test]
    fn test_form_html_escapes_and_shows_errors() {
        let form = FormConfig {
            description: "<b>Hi</b>\nthere".into(),
            submit_label: "Send".into(),
            ..base_form()
        };
        let html = form_html(
            &form,
            "/contact/default",
            &values(&[("name", "\"><script>")]),
            &[FieldError {
                field: "email".into(),
                code: "required".into(),
                message: "\"Email\" is required".into(),
            }],
        );
        assert!(html.contains(r#"action="/contact/default""#));
        assert!(html.contains("&lt;b&gt;Hi&lt;/b&gt;<br>there"));
        assert!(html.contains(r#"value="&quot;&gt;&lt;script&gt;""#));
        assert!(!html.contains("<script>"));
        assert!(html.contains(
            r#"aria-invalid="true" aria-describedby="aegis-contact-default-email-error""#
        ));
        assert!(html.contains(r#"<p class="aegis-contact-error" id="aegis-contact-default-email-error">&quot;Email&quot; is required</p>"#));
        assert!(html.contains(r#"name="website" tabindex="-1""#));
        assert!(html.contains(r#"<textarea id="aegis-contact-default-message" name="message" required maxlength="5000" rows="6"></textarea>"#), "{}", html);
    }

    #[test]
    fn test_sanitize_filename_and_extension() {
        assert_eq!(
            sanitize_filename("../../etc/passwd.txt").unwrap(),
            "passwd.txt"
        );
        assert_eq!(sanitize_filename(r"C:\dir\a:b?.PNG").unwrap(), "a_b_.PNG");
        assert_eq!(sanitize_filename("  ..hidden.. ").unwrap(), "hidden");
        assert_eq!(sanitize_filename("..").as_deref(), None);
        let long = format!("{}.pdf", "x".repeat(300));
        let name = sanitize_filename(&long).unwrap();
        assert_eq!(name.chars().count(), 100);
        assert!(name.ends_with(".pdf"));
        assert_eq!(extension_of("report.TAR.Gz"), "gz");
        assert_eq!(extension_of("noext"), "");
    }

    #[test]
    fn test_form_html_with_file_field_is_multipart() {
        let mut form = base_form();
        assert!(!form_html(&form, "/contact/default", &HashMap::new(), &[]).contains("multipart"));
        let mut file = form.fields[0].clone();
        file.name = "attachment".into();
        file.kind = FieldKind::File;
        form.fields.push(file);
        let html = form_html_with(
            &form,
            "/contact/default",
            &HashMap::new(),
            &[],
            &["pdf".to_string(), "png".to_string()],
        );
        assert!(html.contains(r#"enctype="multipart/form-data""#));
        assert!(html.contains(r#"type="file""#));
        assert!(html.contains(r#"accept=".pdf,.png""#));
    }

    #[test]
    fn test_japanese_messages() {
        let mut form = FormConfig {
            fields: default_fields("ja"),
            ..FormConfig::default()
        };
        let mut agree = form.fields[0].clone();
        agree.name = "agree".into();
        agree.label = "同意する".into();
        agree.kind = FieldKind::Checkbox;
        form.fields.push(agree);
        let errors = validate(&form, &values(&[("email", "x")]), "ja").unwrap_err();
        let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(
            messages,
            vec![
                "「お名前」を入力してください",
                "「メールアドレス」に正しいメールアドレスを入力してください",
                "「お問い合わせ内容」を入力してください",
                "「同意する」にチェックを入れてください",
            ]
        );
        assert_eq!(errors[3].code, "required");
    }
}
