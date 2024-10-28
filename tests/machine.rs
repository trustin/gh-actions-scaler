#[cfg(test)]
mod string_escape {
    use gh_actions_scaler::machine::StringExt;
    use speculoos::assert_that;

    #[test]
    fn no_escape_needed() {
        let mut s = String::new();
        s.push_str_escaped(&"hello");
        assert_that!(s).is_equal_to("hello".to_string());
    }

    #[test]
    fn empty_string() {
        let mut s = String::new();
        s.push_str_escaped("");
        assert_that!(s).is_equal_to("".to_string());
    }

    #[test]
    fn double_quotes() {
        let mut s = String::new();
        s.push_str_escaped(r#"Hello "World""#);
        assert_that!(s).is_equal_to(r#""Hello \"World\"""#.to_string());
    }

    #[test]
    fn only_double_quotes() {
        let mut s = String::new();
        s.push_str_escaped(r#""""#);
        assert_that!(s).is_equal_to(r#""\"\"""#.to_string());
    }

    #[test]
    fn backslash() {
        let mut s = String::new();
        s.push_str_escaped(r"C:\Users\jopopscript");
        assert_that!(s).is_equal_to(r#""C:\\Users\\jopopscript""#.to_string());
    }

    #[test]
    fn only_backslashes() {
        let mut s = String::new();
        s.push_str_escaped(r"\\");
        assert_that!(s).is_equal_to(r#""\\\\""#.to_string());
    }

    #[test]
    fn double_quotes_and_backslash() {
        let mut s = String::new();
        s.push_str_escaped(r#""quoted" \path\"#);
        assert_that!(s).is_equal_to(r#""\"quoted\" \\path\\""#.to_string());
    }

    #[test]
    fn space() {
        let mut s = String::new();
        s.push_str_escaped("Hello World");
        assert_that!(s).is_equal_to(r#""Hello World""#.to_string());
    }

    #[test]
    fn unicode() {
        let mut s = String::new();
        s.push_str_escaped("안녕하세요 \"희승님\"");
        assert_that!(s).is_equal_to(r#""안녕하세요 \"희승님\"""#.to_string());
    }
}
