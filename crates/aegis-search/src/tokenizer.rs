//! 日本語・中国語・韓国語を辞書なしで検索できるようにする分かち書き
//!
//! - ラテン文字・数字などの語（連続した英数字）は、小文字にして 1 語を 1 トークンにする
//! - CJK の文字（漢字・ひらがな・カタカナ・ハングル）が続く部分は、2 文字ずつずらして切る（bigram）。
//!   索引には 1 文字のトークンも同じ位置に入れ、1 文字だけの検索語でも見つかるようにする
//! - それ以外（空白・句読点・記号）は区切り
//!
//! 全角の英数字は半角にしてから扱う（`ＡＥＧＩＳ` と `aegis` は同じ語）

use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

/// tantivy に登録する名前
pub const TOKENIZER_NAME: &str = "aegis_cjk";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CharClass {
    Word,
    Cjk,
    Separator,
}

/// 全角の英数字・記号を半角に、大文字を小文字にする。1 文字が 1 文字になるものだけを変える
pub(crate) fn fold_char(c: char) -> char {
    let c = match c as u32 {
        0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        _ => c,
    };
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

pub(crate) fn classify(c: char) -> CharClass {
    let code = c as u32;
    let cjk = matches!(
        code,
        0x3005..=0x3007          // 々 〆 〇
        | 0x3040..=0x309F        // ひらがな
        | 0x30A0..=0x30FF        // カタカナ（長音符を含む）
        | 0x31F0..=0x31FF        // カタカナの拡張
        | 0x3400..=0x4DBF        // CJK 統合漢字 拡張 A
        | 0x4E00..=0x9FFF        // CJK 統合漢字
        | 0xAC00..=0xD7AF        // ハングル
        | 0xF900..=0xFAFF        // CJK 互換漢字
        | 0xFF66..=0xFF9F        // 半角カタカナ
        | 0x20000..=0x3FFFF // 拡張 B 以降
    );
    if cjk {
        CharClass::Cjk
    } else if c.is_alphanumeric() {
        CharClass::Word
    } else {
        CharClass::Separator
    }
}

/// 同じ種類の文字が続く部分（`(種類, 開始バイト位置, 文字の並び)`）。区切りは含めない
pub(crate) fn runs(text: &str) -> Vec<(CharClass, Vec<(usize, char)>)> {
    let mut out: Vec<(CharClass, Vec<(usize, char)>)> = Vec::new();
    let mut prev = CharClass::Separator;
    for (offset, c) in text.char_indices() {
        let class = classify(fold_char(c));
        if class == CharClass::Separator {
            prev = class;
            continue;
        }
        if class == prev {
            if let Some((_, chars)) = out.last_mut() {
                chars.push((offset, c));
                continue;
            }
        }
        out.push((class, vec![(offset, c)]));
        prev = class;
    }
    out
}

/// トークンに分ける。`index` なら索引用（CJK の 1 文字のトークンも入れる）、そうでなければ検索語用
/// （CJK は 2 文字以上なら bigram だけ、1 文字ならその 1 文字）
pub fn tokenize(text: &str, index: bool) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut position = 0usize;
    for (class, chars) in runs(text) {
        let end_of = |i: usize| chars[i].0 + chars[i].1.len_utf8();
        match class {
            CharClass::Word => {
                tokens.push(Token {
                    offset_from: chars[0].0,
                    offset_to: end_of(chars.len() - 1),
                    position,
                    text: chars.iter().map(|&(_, c)| fold_char(c)).collect(),
                    position_length: 1,
                });
                position += 1;
            }
            CharClass::Cjk => {
                for i in 0..chars.len() {
                    let single = chars.len() == 1;
                    if index || single {
                        tokens.push(Token {
                            offset_from: chars[i].0,
                            offset_to: end_of(i),
                            position,
                            text: fold_char(chars[i].1).to_string(),
                            position_length: 1,
                        });
                    }
                    if i + 1 < chars.len() {
                        tokens.push(Token {
                            offset_from: chars[i].0,
                            offset_to: end_of(i + 1),
                            position,
                            text: [fold_char(chars[i].1), fold_char(chars[i + 1].1)]
                                .iter()
                                .collect(),
                            position_length: 1,
                        });
                    }
                    // 検索語でも 1 文字ごとに位置を進め、続く語の位置を索引と揃える
                    position += 1;
                }
            }
            CharClass::Separator => {}
        }
    }
    tokens
}

/// 索引用の分かち書き（tantivy の `Tokenizer`）
#[derive(Debug, Clone, Default)]
pub struct CjkBigramTokenizer;

pub struct VecTokenStream {
    tokens: Vec<Token>,
    next: usize,
}

impl TokenStream for VecTokenStream {
    fn advance(&mut self) -> bool {
        if self.next < self.tokens.len() {
            self.next += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.tokens[self.next - 1]
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.next - 1]
    }
}

impl Tokenizer for CjkBigramTokenizer {
    type TokenStream<'a> = VecTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> VecTokenStream {
        VecTokenStream {
            tokens: tokenize(text, true),
            next: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(tokens: &[Token]) -> Vec<(String, usize)> {
        tokens
            .iter()
            .map(|t| (t.text.clone(), t.position))
            .collect()
    }

    #[test]
    fn test_latin_words_are_lowercased() {
        assert_eq!(
            texts(&tokenize("Hello, AegisCMS 2026!", true)),
            vec![
                ("hello".into(), 0),
                ("aegiscms".into(), 1),
                ("2026".into(), 2)
            ]
        );
    }

    #[test]
    fn test_full_width_alphanumerics_are_folded() {
        assert_eq!(
            texts(&tokenize("ＡｅｇｉｓＣＭＳ", false)),
            vec![("aegiscms".into(), 0)]
        );
    }

    #[test]
    fn test_cjk_index_tokens_have_unigrams_and_bigrams() {
        assert_eq!(
            texts(&tokenize("東京都", true)),
            vec![
                ("東".into(), 0),
                ("東京".into(), 0),
                ("京".into(), 1),
                ("京都".into(), 1),
                ("都".into(), 2)
            ]
        );
    }

    #[test]
    fn test_cjk_query_tokens_are_bigrams_only() {
        assert_eq!(
            texts(&tokenize("東京都", false)),
            vec![("東京".into(), 0), ("京都".into(), 1)]
        );
        assert_eq!(texts(&tokenize("猫", false)), vec![("猫".into(), 0)]);
    }

    #[test]
    fn test_mixed_script_positions_line_up_between_index_and_query() {
        // 検索語のトークンの位置の差が、索引の中の位置の差と同じになる（フレーズで一致させるため）
        let index = tokenize("AegisCMSの使い方", true);
        let query = tokenize("AegisCMSの使い方", false);
        for q in &query {
            assert!(
                index
                    .iter()
                    .any(|t| t.text == q.text && t.position == q.position),
                "{:?} not in index tokens",
                q
            );
        }
        let query = tokenize("使い方Rust", false);
        assert_eq!(
            texts(&query),
            vec![("使い".into(), 0), ("い方".into(), 1), ("rust".into(), 3)]
        );
        assert_eq!(
            texts(&tokenize("AegisCMSの使い方", false)),
            vec![
                ("aegiscms".into(), 0),
                ("の使".into(), 1),
                ("使い".into(), 2),
                ("い方".into(), 3)
            ]
        );
    }

    #[test]
    fn test_offsets_point_at_the_original_text() {
        let text = "Rust で書いた";
        for t in tokenize(text, true) {
            let original: String = text[t.offset_from..t.offset_to]
                .chars()
                .map(fold_char)
                .collect();
            assert_eq!(original, t.text);
        }
    }

    #[test]
    fn test_punctuation_separates() {
        assert_eq!(
            texts(&tokenize("「検索」、です。", false)),
            vec![("検索".into(), 0), ("です".into(), 2)]
        );
    }
}
