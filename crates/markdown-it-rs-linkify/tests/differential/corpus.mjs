// Systematic grammar coverage supplements mutations of the short snapshots.
const hosts = [
  "example.com", "example.c", "example.or", "localhost", "a", "9",
  "例子.测试", "😀.com", "é.org", "a..com", "a.-com", "-a.com",
  "a-.com", "a_b.com", "a+b.com", "a|b.com", "a`b.com", "a$b.com",
  "xn--fiqs8s.cn", "xn--a", "xn--", "192.168.0.1", "256.168.0.1",
  "[::1]", "[2001:db8::1]", "[::ffff:192.168.0.1]", "[:::1]",
  "[IPv6:::1]", "user:pass@example.org", "a.com:0", "a.com:65535",
  "a.com:65536", "a.com:00001", "a.com:999999", "a.com:abc",
  "a.com.", "a.com-", "a.com_", "a.рф", "a.РФ", "a.xn--p1ai",
  "a.com𐄀next", "𐄀.example.com", "a.𐄀", "a𑁇b.com",
];
const tails = [
  "", "/", "?", "#", "/path", "/路径😀", "/a_(b)", "/a_(b",
  "/a_[b]", "/a_[b", "/a_{b}", "/a_{b", "/a_((b))",
  "/a_(((((b)))))", "/a., next", "/a...b", "/a...:b", "/a??b",
  "/a!!b", "/a!!", "/a;", "/a;b", "/a,b", "/a｜text",
  "/a|b`c", '/a"b c"', '/a"bc"', "/a'bc'", "/a'b", "/a@b.co",
  "/a\u0000b", "/a\nb", "/a\\b", "/a---b", "/a>next",
];
const boundaries = [
  "", " ", "\n", "\u0000", "x", "_", ":", "/", "\\", "-", ".",
  "@", "(", ")", "[", "{", "'", '"', "`", "|", "｜", "$", "+", "=",
  "^", "中文", "😀", "。", "é", "\u200b", "\u00a0",
];
const schemas = ["http://", "HTTPS://", "ftp://", "//", ""];
const names = ["a", "a.b", ".a", "a.", "a..b", "foo+special", "a/b",
  "é", "😀", "a_b", "a`b", "a|b", "a".repeat(64), "a".repeat(65), "a".repeat(100)];

export function* structuredCases() {
  let index = 0;
  function fixture(source, fuzzyLinks) {
    return { id: `structured-${index++}`, request: { source, fuzzyLinks } };
  }
  for (const schema of schemas) for (const host of hosts) for (const tail of tails) {
    for (const fuzzy of [false, true]) yield fixture(schema + host + tail, fuzzy);
  }
  for (const boundary of boundaries) for (const schema of schemas) for (const host of hosts) {
    yield fixture(boundary + schema + host + " next@example.com", true);
  }
  for (const boundary of boundaries) for (const name of names) for (const host of hosts) {
    yield fixture(boundary + name + "@" + host, false);
    yield fixture(boundary + "mailto:" + name + "@" + host, true);
  }
  for (const schema of ["https:", "http:", "mailto:", "custom:"]) {
    for (const prefix of ["", "https:", "mailto:", "x", " ", "://"]) {
      yield fixture(prefix + schema + "//example.org/a@b.co next@example.com", true);
    }
  }
  for (const size of [62, 63, 64, 65, 100, 999, 1000, 1001, 9998, 9999, 10000, 10001, 20000]) {
    for (const ch of ["a", "é", "😀"]) {
      yield fixture("https://" + ch.repeat(size) + ".com/path", true);
      yield fixture("https://a.com/(" + ch.repeat(size) + ") next@example.com", false);
      // The ASCII prefix uses an even number of validator units, so a length
      // cap here never exports an index inside an astral scalar. The Rust
      // unit tests separately exercise that unrepresentable JS edge case.
      yield fixture("https://a.com/" + ch.repeat(size) + "abc next@example.com", false);
    }
  }
}
