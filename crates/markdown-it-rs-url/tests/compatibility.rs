//! Expected outputs captured from markdown-it 15.0.2 (mdurl 2.1.0, punycode.js 2.3.1).
use markdown_it_rs_url::{decode_url_for_display, format_url_for_computers};

#[test]
fn markdown_it_url_regressions() {
    for (input, link, text) in [
        (
            "httpstps://e�ame.com?find=\\*",
            "httpstps://e%EF%BF%BDame.com?find=%5C*",
            "httpstps://e�ame.com?find=\\*",
        ),
        (
            "httptp://☃.nep://☃.ne]/",
            "httptp://%E2%98%83.nep://%E2%98%83.ne%5D/",
            "httptp://☃.nep://☃.ne]/",
        ),
        (
            "http://xn--xnbé|9/",
            "http://xn--xn--xnb-hya%7C9/",
            "http://xn--xnbé|9/",
        ),
        ("http://xn--γ`", "http://xn--xn---emd%60", "http://xn--γ`"),
        ("http://[200]:`<", "http://200:%60%3C", "http://200:`<"),
        (
            "httper:password@[2001:db8:0:0:0:0:0:1]:1926/",
            "httper:password@[2001:db8:0:0:0:0:0:1]:1926/",
            "httper:password@[2001:db8:0:0:0:0:0:1]:1926/",
        ),
        (
            "http://user:password@[2001:db8:0:0😀9:0:0:1]:1926/",
            "http://user:password@[xn--2001:db8:0:09:0:0:1-wc01s]:1926/",
            "http://user:password@[2001:db8:0:0😀9:0:0:1]:1926/",
        ),
        ("http://É.com", "http://xn--dca.com", "http://É.com"),
        (
            "http://e\u{301}.com",
            "http://xn--e-xbb.com",
            "http://e\u{301}.com",
        ),
        (
            "http://ＦＯＯ.com",
            "http://xn--uh7csaa.com",
            "http://ＦＯＯ.com",
        ),
    ] {
        assert_eq!(format_url_for_computers(input), link, "normalize {input:?}");
        assert_eq!(decode_url_for_display(input), text, "display {input:?}");
    }
}
