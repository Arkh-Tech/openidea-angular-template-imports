//! Syntactic candidates only. The *meaning* comes from Angular declarations.
//! We intentionally never propose imports for custom element tags.
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Need {
    Directive { name: String, tag: String, attrs: Vec<String> },
    Pipe(String),
}
impl Need {
    pub fn id(&self) -> String {
        match self {
            Self::Directive { name, tag, .. } => format!("directive:{tag}:{name}"),
            Self::Pipe(name) => format!("pipe:{name}"),
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Directive { name, .. } => format!("diretiva '{name}'"),
            Self::Pipe(name) => format!("pipe '{name}'"),
        }
    }
}
fn push(into: &mut BTreeMap<String, Need>, need: Need) { into.insert(need.id(), need); }

fn normalize_attr(raw: &str) -> Option<&str> {
    if raw.starts_with('(') && !raw.starts_with("[(") { return None; } // DOM output events
    if raw.starts_with('#') || raw.starts_with("let-") || raw.starts_with("on-") { return None; }
    let clean = raw.trim_start_matches("[(").trim_end_matches(")]")
        .trim_start_matches('[').trim_end_matches(']')
        .trim_start_matches('*').trim_start_matches("bind-");
    if clean.is_empty() || clean.starts_with("attr.") || clean.starts_with("style.")
        || clean.starts_with("class.") || clean.starts_with('@') { return None; }
    if clean.starts_with("data-") || clean.starts_with("aria-") || clean.starts_with("i18n-") { return None; }
    // Native HTML attributes/properties don't imply an Angular directive.
    // This is a noise filter, *not* a directive->import mapping.
    const HTML: &[&str] = &[
        "id","class","style","href","target","rel","type","name","value","title",
        "placeholder","disabled","required","readonly","checked","selected","multiple",
        "src","srcset","alt","width","height","download","role","tabindex","for",
        "autofocus","hidden","accept","min","max","step","minlength","maxlength",
        "pattern","autocomplete","autocapitalize","action","method","enctype",
        "cols","rows","rowspan","colspan","scope","headers","dir","lang","charset",
        "content","http-equiv","media","sizes","loading","decoding","fetchpriority",
        "draggable","spellcheck","contenteditable","translate","open","reversed",
        "start","wrap","form","list","poster","preload","controls","autoplay",
        "loop","muted","playsinline","crossorigin","referrerpolicy","slot","part",
        "as","async","defer","nonce","integrity","ping","accept-charset",
        "textContent","innerHTML","innerText","offsetWidth","offsetHeight","scrollTop",
        "ngProjectAs","ngNonBindable","i18n","ng-container",
    ];
    if HTML.iter().any(|x| x.eq_ignore_ascii_case(clean)) { return None; }
    Some(clean)
}

pub fn detect(html: &str) -> Vec<Need> {
    let mut found = BTreeMap::<String, Need>::new();
    let b = html.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"<!--") {
            i = b[i + 4..].windows(3).position(|w| w == b"-->")
                .map(|j| i + 4 + j + 3).unwrap_or(b.len());
            continue;
        }
        if b[i..].starts_with(b"{{") {
            let end = b[i + 2..].windows(2).position(|w| w == b"}}")
                .map(|j| i + 2 + j).unwrap_or(b.len());
            pipes(&html[i + 2..end], &mut found);
            i = (end + 2).min(b.len());
            continue;
        }
        if b[i] == b'<' && i + 1 < b.len() && b[i + 1].is_ascii_alphabetic() {
            let start = i + 1;
            let mut end_tag = start;
            while end_tag < b.len() && (b[end_tag].is_ascii_alphanumeric() || b"-_:".contains(&b[end_tag])) { end_tag += 1; }
            let tag = html[start..end_tag].to_ascii_lowercase();
            let mut p = end_tag;
            let mut attrs = Vec::<String>::new();
            let mut candidates = Vec::<String>::new();
            loop {
                while p < b.len() && b[p].is_ascii_whitespace() { p += 1; }
                if p >= b.len() || b[p] == b'>' { break; }
                if b[p] == b'/' { p += 1; continue; }
                let begin = p;
                while p < b.len() && !b[p].is_ascii_whitespace() && b[p] != b'=' && b[p] != b'>' { p += 1; }
                if begin == p { p += 1; continue; }
                let attr = &html[begin..p];
                // Every attribute participates in compound CSS selectors, including native ones.
                let selector_name = attr.trim_start_matches("[(").trim_end_matches(")]")
                    .trim_start_matches('[').trim_end_matches(']')
                    .trim_start_matches('*').trim_start_matches("bind-");
                attrs.push(selector_name.to_owned());
                if let Some(name) = normalize_attr(attr) { candidates.push(name.to_owned()); }
                while p < b.len() && b[p].is_ascii_whitespace() { p += 1; }
                if p < b.len() && b[p] == b'=' {
                    p += 1;
                    while p < b.len() && b[p].is_ascii_whitespace() { p += 1; }
                    if p < b.len() && (b[p] == b'\'' || b[p] == b'"') {
                        let quote = b[p]; p += 1;
                        let value_start = p;
                        while p < b.len() && b[p] != quote { p += 1; }
                        if attr.starts_with('[') || attr.starts_with('*') || attr.starts_with("bind-") {
                            pipes(&html[value_start..p], &mut found);
                        }
                        p = (p + 1).min(b.len());
                    } else {
                        let value_start = p;
                        while p < b.len() && !b[p].is_ascii_whitespace() && b[p] != b'>' { p += 1; }
                        if attr.starts_with('[') || attr.starts_with('*') || attr.starts_with("bind-") {
                            pipes(&html[value_start..p], &mut found);
                        }
                    }
                }
            }
            for name in candidates {
                push(&mut found, Need::Directive { name, tag: tag.clone(), attrs: attrs.clone() });
            }
            i = (p + 1).min(b.len());
            continue;
        }
        i += 1;
    }
    found.into_values().collect()
}
fn pipes(expr: &str, into: &mut BTreeMap<String, Need>) {
    // Scans Angular interpolation and bound expressions for a pipe token.
    // Ignore ||, and pipes in quoted strings.
    let b = expr.as_bytes();
    let mut quote = 0u8;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == b'\\' { i += 2; continue; }
            if c == quote { quote = 0; }
            i += 1; continue;
        }
        if matches!(c, b'\'' | b'"' | b'`') { quote = c; i += 1; continue; }
        if c != b'|' || (i > 0 && b[i-1] == b'|') || b.get(i+1) == Some(&b'|') { i += 1; continue; }
        let mut j = i + 1;
        while j < b.len() && b[j].is_ascii_whitespace() { j += 1; }
        let start = j;
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'$') { j += 1; }
        if j > start && (b[start].is_ascii_alphabetic() || b[start] == b'_' || b[start] == b'$') {
            push(into, Need::Pipe(expr[start..j].to_owned()));
        }
        i = j.max(i+1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidates_are_generic_and_ignore_component_tags() {
        let needs = detect(r#"<app-widget [formField]="control" routerLink="/go"></app-widget><input mask="xx">{{x | mask: '00'}}"#);
        assert!(needs.iter().any(|x| matches!(x, Need::Directive{name, ..} if name=="formField")));
        assert!(needs.iter().any(|x| matches!(x, Need::Directive{name, ..} if name=="routerLink")));
        assert!(needs.iter().any(|x| matches!(x, Need::Directive{name, ..} if name=="mask")));
        assert!(needs.contains(&Need::Pipe("mask".into())));
        assert!(!needs.iter().any(|x| x.label().contains("app-widget")));
    }
    #[test]
    fn avoids_native_properties_comments_and_logical_or() {
        let needs = detect(r#"<!-- <div customThing></div> --><input [value]="x || y" (click)="go()" class="ok" [formField]="ctrl"/>"#);
        assert_eq!(needs.len(), 1);
        assert!(matches!(&needs[0], Need::Directive{name, ..} if name == "formField"));
    }
}
