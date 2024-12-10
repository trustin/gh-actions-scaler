#[cfg(test)]
mod string_ext_tests {
    use gh_actions_scaler::machine::StringExt;
    use speculoos::assert_that;
    use test_case::test_case;

    #[test_case("", ""; "empty string")]
    #[test_case("hello", "hello"; "a single word")]
    #[test_case("안녕하세요", "안녕하세요"; "a single word (unicode)")]
    #[test_case("Hello, World", r#""Hello, World""#; "two words")]
    #[test_case("안녕하세요, 여러분!", r#""안녕하세요, 여러분!""#; "two words (unicode)")]
    #[test_case(r#""foo"bar"baz""#, r#""\"foo\"bar\"baz\"""#; "double quotes")]
    #[test_case("'foo'bar'baz'", r#""'foo'bar'baz'""#; "single quotes")]
    #[test_case(r"\foo\bar\baz\", r#""\\foo\\bar\\baz\\""#; "backslashes")]
    #[test_case(r#""foo" \bar\ 'baz'"#, r#""\"foo\" \\bar\\ 'baz'""#; "mixed special characters")]
    fn push_str_escaped(input: &str, expected: &str) {
        let mut actual = String::new();
        actual.push_str_escaped(input);
        assert_that!(actual).is_equal_to(expected.to_string());
    }

    #[test]
    fn push_str_escaped_special_chars() {
        let chars_to_escape = "'|&!;$()[]{}<>#`";
        for char_to_escape in chars_to_escape.chars() {
            let input = format!(
                "{}foo{}bar{}",
                char_to_escape, char_to_escape, char_to_escape
            );
            let expected = format!("\"{}\"", input);
            let mut actual = String::new();
            actual.push_str_escaped(&input);
            assert_that!(actual).is_equal_to(expected);
        }
    }
}
