//! Minimal, conservative reader of Angular's published Ivy .d.ts metadata.
//! This is not a TypeScript parser. Unsupported declarations/selectors are skipped.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Directive, Pipe, Module }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decl {
    pub kind: Kind,
    pub symbol: String,
    pub name: String,  // directive CSS selector or pipe name
    pub standalone: bool,
    pub exported_symbols: Vec<String>, // NgModule export list
}

pub fn quoted(s: &str) -> Option<String> {
    let s = s.trim_start();
    let q = s.as_bytes().first().copied()?;
    if q != b'\'' && q != b'"' { return None; }
    let end = s[1..].find(q as char)?;
    Some(s[1..1+end].to_owned())
}
fn ident(s: &str) -> String {
    s.trim().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
        .collect()
}
fn generic_args(s: &str, start: usize) -> Option<Vec<String>> {
    let bytes = s.as_bytes();
    let at = start + s[start..].find('<')?;
    if at - start > 10 { return None; }
    let mut angle = 1i32;
    let (mut square, mut brace, mut round) = (0i32, 0i32, 0i32);
    let mut quote = 0u8;
    let mut last = at + 1;
    let mut args = Vec::new();
    let mut i = at + 1;
    while i < bytes.len() {
        let c = bytes[i];
        if quote != 0 {
            if c == b'\\' { i += 2; continue; }
            if c == quote { quote = 0; }
            i += 1; continue;
        }
        match c {
            b'\'' | b'"' | b'`' => quote = c,
            b'<' => angle += 1,
            b'>' => {
                angle -= 1;
                if angle == 0 {
                    args.push(s[last..i].trim().to_owned());
                    return Some(args);
                }
            }
            b'[' => square += 1, b']' => square -= 1,
            b'{' => brace += 1, b'}' => brace -= 1,
            b'(' => round += 1, b')' => round -= 1,
            b',' if angle == 1 && square == 0 && brace == 0 && round == 0 => {
                args.push(s[last..i].trim().to_owned());
                last = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    None
}
fn typeof_symbols(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in s.split("typeof ").skip(1) {
        let name = ident(part);
        if !name.is_empty() && !out.contains(&name) { out.push(name); }
    }
    out
}
pub fn declarations(text: &str) -> Vec<Decl> {
    const KINDS: &[(&str,Kind)] = &[
        ("ɵɵDirectiveDeclaration", Kind::Directive),
        ("ɵɵPipeDeclaration", Kind::Pipe),
        ("ɵɵNgModuleDeclaration", Kind::Module),
    ];
    let mut result = Vec::new();
    for (needle, kind) in KINDS {
        for (at, _) in text.match_indices(*needle) {
            let Some(args) = generic_args(text, at + needle.len()) else { continue };
            let Some(symbol) = args.first().map(|x| ident(x)) else { continue };
            if symbol.is_empty() { continue; }
            let (name, standalone, exports) = match kind {
                Kind::Directive if args.len() >= 8 => {
                    let Some(selector) = quoted(&args[1]) else { continue; };
                    (selector, args[7].trim() == "true", Vec::new())
                }
                Kind::Pipe if args.len() >= 3 => {
                    let Some(name) = quoted(&args[1]) else { continue; };
                    (name, args[2].trim() == "true", Vec::new())
                }
                Kind::Module if args.len() >= 4 => {
                    (String::new(), false, typeof_symbols(&args[3]))
                }
                _ => continue,
            };
            result.push(Decl { kind: *kind, symbol, name, standalone, exported_symbols: exports });
        }
    }
    result
}

#[derive(Debug)]
pub struct Export {
    pub path: Option<String>,
    /// Symbol inside target -> public alias inside this file.
    pub symbols: BTreeMap<String, BTreeSet<String>>,
    pub star: bool,
}

/// `export { Foo, Bar as Baz } from './entry.js'` and `export * from`.
/// Also `export { Foo }` (local). Supports multiline syntax.
pub fn exports(text: &str) -> Vec<Export> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(p) = text[start..].find("export ") {
        let at = start + p + "export ".len();
        let tail = text[at..].trim_start();
        let prefix = text.len() - tail.len();
        if tail.starts_with('{') {
            let Some(end_rel) = tail.find('}') else { break; };
            let inner = &tail[1..end_rel];
            let mut symbols = BTreeMap::new();
            for spec in inner.split(',') {
                let spec = spec.trim();
                let spec = spec.strip_prefix("type ").unwrap_or(spec);
                let tokens: Vec<_> = spec.split_whitespace().collect();
                if tokens.is_empty() { continue; }
                let alias = if tokens.len() >= 3 && tokens[1] == "as" { tokens[2] } else { tokens[0] };
                symbols.entry(tokens[0].to_owned())
                    .or_insert_with(BTreeSet::new).insert(alias.to_owned());
            }
            let rest = tail[end_rel+1..].trim_start();
            let path = rest.strip_prefix("from").and_then(|s| quoted(s));
            out.push(Export { path, symbols, star: false });
            start = prefix + end_rel + 1;
        } else if tail.starts_with('*') {
            let rest = tail[1..].trim_start();
            if let Some(path) = rest.strip_prefix("from").and_then(quoted) {
                out.push(Export { path: Some(path), symbols: BTreeMap::new(), star: true });
            }
            start = (prefix + 1).min(text.len());
        } else {
            start = at;
        }
        if start >= bytes.len() { break; }
    }
    out
}

/// Only match attribute selectors, never component element selectors.
/// Supports `[x]`, `input[x]`, `[x][y]`, `input[x], textarea[x]` and `:not([y])`.
/// Returns false on unsupported CSS constructs rather than guessing.
pub fn selector_matches(selector: &str, tag: &str, attrs: &[String], target: &str) -> bool {
    selector.split(',').any(|alt| selector_part(alt.trim(), tag, attrs, target))
}
fn selector_part(part: &str, tag: &str, attrs: &[String], target: &str) -> bool {
    if part.is_empty() { return false; }
    let mut base = part.to_owned();
    let mut nots = Vec::new();
    while let Some(pos) = base.find(":not(") {
        let after = pos + 5;
        let Some(end_rel) = base[after..].find(')') else { return false; };
        let end = after + end_rel;
        nots.push(base[after..end].to_owned());
        base.replace_range(pos..=end, "");
    }
    // CSS :not(a) and :not(area) match *element names*, not attributes.
    // Previously `basic_match` required an attribute and silently ignored
    // these negations, making legacy RouterLink and RouterLinkWithHref both
    // appear valid for the same anchor tag.
    if nots.iter().any(|n| negation_matches(n, tag, attrs)) { return false; }
    basic_match(&base, tag, attrs, Some(target))
}
fn negation_matches(negated: &str, tag: &str, attrs: &[String]) -> bool {
    let negated = negated.trim();
    if !negated.is_empty() && negated.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
        return negated.eq_ignore_ascii_case(tag);
    }
    // Attribute negations such as :not([disabled]) still work.
    basic_match(negated, tag, attrs, None)
}
fn basic_match(part: &str, tag: &str, attrs: &[String], target: Option<&str>) -> bool {
    let part = part.trim();
    let element = part.split('[').next().unwrap_or("").trim();
    if !element.is_empty() && element != "*" && element != tag { return false; }
    let mut names = Vec::new();
    let mut cursor = part[element.len()..].trim();
    while !cursor.is_empty() {
        let Some(after_open) = cursor.strip_prefix('[') else { return false; };
        let Some(end) = after_open.find(']') else { return false; };
        let inside = after_open[..end].trim();
        if inside.contains('=') || inside.contains('~') || inside.contains('^')
            || inside.contains('$') || inside.contains('|') { return false; }
        if inside.is_empty() { return false; }
        names.push(inside);
        cursor = after_open[end+1..].trim();
    }
    !names.is_empty()
        && target.is_none_or(|needle| names.contains(&needle))
        && names.iter().all(|name| attrs.iter().any(|a| a == name))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_ivy_directives_pipes_and_modules() {
        let text = r#"
        static ɵdir: i0.ɵɵDirectiveDeclaration<RouterLink, "[routerLink]", never, { "routerLink": {"alias":"routerLink";} }, {}, never, never, true, never>;
        static ɵpipe: i0.ɵɵPipeDeclaration<NgxMaskPipe, "mask", true>;
        static ɵmod: i0.ɵɵNgModuleDeclaration<FormsModule, [typeof NgModel], never, [typeof NgModel]>;"#;
        let out = declarations(text);
        assert_eq!(out.len(),3);
        assert_eq!(out[0].name,"[routerLink]");
        assert_eq!(out[1].name,"mask");
        assert_eq!(out[2].exported_symbols,vec!["NgModel"]);
    }
    #[test]
    fn parses_reexports() {
        let out = exports("export { RouterLink, NgModel as Model } from './some.d.js';\nexport * from './other.js';");
        assert!(out[0].symbols["NgModel"].contains("Model"));
        assert_eq!(out[0].path.as_deref(),Some("./some.d.js"));
        assert!(out[1].star);
    }
    #[test]
    fn preserves_multiple_public_names_for_one_declaration() {
        // Angular can expose RouterLink and its deprecated RouterLinkWithHref
        // alias from the *same* underlying declaration.
        let reexports = exports(
            "export { RouterLink, RouterLink as RouterLinkWithHref } from './router_link.d.js';"
        );
        let public = &reexports[0].symbols["RouterLink"];
        assert_eq!(public.len(), 2);
        assert!(public.contains("RouterLink"));
        assert!(public.contains("RouterLinkWithHref"));
    }
    #[test]
    fn distinguishes_legacy_routerlink_directives_on_anchor_elements() {
        // Older Angular versions exposed separate directives for <a>/<area>
        // and other tags, with complementary CSS selectors.
        let attrs = vec!["routerLink".into()];
        assert!(!selector_matches(":not(a):not(area)[routerLink]", "a", &attrs, "routerLink"));
        assert!(!selector_matches(":not(a):not(area)[routerLink]", "area", &attrs, "routerLink"));
        assert!(selector_matches(":not(a):not(area)[routerLink]", "button", &attrs, "routerLink"));
        assert!(selector_matches("a[routerLink], area[routerLink]", "a", &attrs, "routerLink"));
        assert!(selector_matches("a[routerLink], area[routerLink]", "area", &attrs, "routerLink"));
        assert!(!selector_matches("a[routerLink], area[routerLink]", "button", &attrs, "routerLink"));
    }
    #[test]
    fn still_rejects_excluded_attribute_selector() {
        let attrs = vec!["routerLink".into(), "disabled".into()];
        assert!(!selector_matches("[routerLink]:not([disabled])", "a", &attrs, "routerLink"));
        assert!(selector_matches("[routerLink]:not([disabled])", "a", &["routerLink".into()], "routerLink"));
    }
    #[test]
    fn matches_selector_without_false_positive() {
        let attrs = vec!["mask".into(), "formField".into()];
        assert!(selector_matches("input[mask],textarea[mask]", "input", &attrs, "mask"));
        assert!(!selector_matches("textarea[mask]", "input", &attrs, "mask"));
        assert!(selector_matches("[formField]", "input", &attrs, "formField"));
        assert!(!selector_matches("input[mask]", "input", &attrs, "formField"));
        assert!(!selector_matches("[mask]:not([formField])", "input", &attrs, "mask"));
    }
}
