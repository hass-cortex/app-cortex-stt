use super::{Locale, Renderer};

#[test]
fn parses_the_subtag_shapes_that_matter() {
    let l = Locale::parse("zh-TW");
    assert_eq!((l.base(), l.script(), l.region()), ("zh", None, Some("TW")));

    let l = Locale::parse("zh-Hant");
    assert_eq!(
        (l.base(), l.script(), l.region()),
        ("zh", Some("Hant"), None)
    );

    let l = Locale::parse("zh-Hant-TW");
    assert_eq!(
        (l.base(), l.script(), l.region()),
        ("zh", Some("Hant"), Some("TW"))
    );

    // Case and separator are the caller's business, not ours.
    let l = Locale::parse("ZH_hant_tw");
    assert_eq!(
        (l.base(), l.script(), l.region()),
        ("zh", Some("Hant"), Some("TW"))
    );
}

#[test]
fn a_bare_base_code_is_the_opt_out() {
    assert!(!Locale::parse("zh").has_subtags());
    assert!(Renderer::for_language(Some("zh")).is_none());
    assert!(Renderer::for_language(None).is_none());
}

#[test]
fn a_language_with_no_module_renders_nothing() {
    // Not an error: an unhandled language means the transcript is returned
    // as the model wrote it.
    assert!(Renderer::for_language(Some("de-CH")).is_none());
    assert!(Renderer::for_language(Some("en-GB")).is_none());
}

#[test]
fn an_unrecognised_chinese_region_renders_nothing() {
    assert!(Renderer::for_language(Some("zh-XX")).is_none());
}

#[test]
fn region_selects_the_variant_within_a_script() {
    let tw = Renderer::for_language(Some("zh-TW")).expect("zh-TW renders");
    let hk = Renderer::for_language(Some("zh-HK")).expect("zh-HK renders");
    let hant = Renderer::for_language(Some("zh-Hant")).expect("zh-Hant renders");

    // The whole reason both spellings are accepted: they differ.
    assert_eq!(tw.render("裏面"), "裡面");
    assert_eq!(hant.render("裏面"), "裏面");
    assert_eq!(tw.render("面条"), "麵條");
    assert_eq!(hk.render("面条"), "麪條");
}

#[test]
fn region_wins_over_script_when_both_are_present() {
    let a = Renderer::for_language(Some("zh-Hant-TW")).expect("renders");
    let b = Renderer::for_language(Some("zh-TW")).expect("renders");
    assert_eq!(a.render("裏面"), b.render("裏面"));
    assert_eq!(a.ids(), vec!["script:s2tw"]);
}

#[test]
fn simplified_is_reachable_so_the_parameter_works_both_ways() {
    let cn = Renderer::for_language(Some("zh-CN")).expect("zh-CN renders");
    assert_eq!(cn.render("關閉入口燈"), "关闭入口灯");
    assert_eq!(cn.ids(), vec!["script:t2s"]);
}

#[test]
fn renders_this_deployments_commands() {
    let r = Renderer::for_language(Some("zh-TW")).expect("renders");
    assert_eq!(r.render("关闭入口灯"), "關閉入口燈");
    assert_eq!(r.render("今天天气"), "今天天氣");
    assert_eq!(r.render("窗帘打开百分之五十"), "窗簾打開百分之五十");
}

/// The phrase-level OpenCC configs rewrite verbs — `打开` becomes `開啟` —
/// which silently breaks a consumer matching on literal phrasing. Only the
/// glyph-level configs are reachable, and this locks that in.
#[test]
fn rendering_never_rewrites_vocabulary() {
    let r = Renderer::for_language(Some("zh-TW")).expect("renders");
    assert_eq!(r.render("打开入口灯"), "打開入口燈");
    assert_eq!(r.render("软件"), "軟件"); // not 軟體
    assert_eq!(r.render("鼠标"), "鼠標"); // not 滑鼠
}

#[test]
fn text_with_no_han_characters_is_untouched() {
    let r = Renderer::for_language(Some("zh-TW")).expect("renders");
    assert_eq!(r.render("alexa"), "alexa");
    assert_eq!(r.render(""), "");
}

#[test]
fn ids_name_what_was_applied() {
    assert_eq!(
        Renderer::for_language(Some("zh-TW")).unwrap().ids(),
        vec!["script:s2tw"]
    );
    assert_eq!(
        Renderer::for_language(Some("zh-Hant")).unwrap().ids(),
        vec!["script:s2t"]
    );
}
