//! Conservative TypeScript decorator/import edits; never rewrites the whole file from a regex.
use std::collections::{BTreeMap, BTreeSet};
use crate::angular::Dependency;

#[derive(Clone, Debug)]
pub struct ComponentMeta {
    pub template_url: Option<String>,
    pub standalone: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct DirectiveMeta {
    pub selector: String,
    pub class_name: Option<String>,
    pub standalone: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct PipeMeta {
    pub name: String,
    pub class_name: Option<String>,
    pub standalone: Option<bool>,
}

fn ident(b: u8) -> bool { b.is_ascii_alphanumeric() || b == b'_' || b == b'$' }
fn keyword_at(src: &str, word: &str, at: usize) -> bool {
    src.as_bytes().get(at..at + word.len()) == Some(word.as_bytes())
        && (at == 0 || !ident(src.as_bytes()[at - 1]))
        && (at + word.len() == src.len() || !ident(src.as_bytes()[at + word.len()]))
}
fn word_in(src: &str, word: &str) -> bool {
    src.match_indices(word).any(|(i, _)| keyword_at(src, word, i))
}

// Same UTF-8 byte length as input, but strings and comments become spaces.
// This permits finding decorators/brackets without being confused by strings.
fn mask(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        if bytes[i..].starts_with(b"//") {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
        } else if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i + 1 < bytes.len() && !bytes[i..].starts_with(b"*/") { i += 1; }
            i = (i + 2).min(bytes.len());
        } else if matches!(bytes[i], b'\'' | b'"' | b'`') {
            let q = bytes[i]; i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' { i = (i + 2).min(bytes.len()); continue; }
                if bytes[i] == q { i += 1; break; }
                i += 1;
            }
        } else { i += 1; continue; }
        for x in &mut out[start..i] { if *x != b'\n' && *x != b'\r' { *x = b' '; } }
    }
    String::from_utf8(out).expect("mask preserves UTF-8")
}
fn matching(s: &str, open: usize, left: u8, right: u8) -> Option<usize> {
    let mut d = 0usize;
    for (delta, &c) in s.as_bytes()[open..].iter().enumerate() {
        if c == left { d += 1; }
        if c == right {
            d = d.checked_sub(1)?;
            if d == 0 { return Some(open + delta); }
        }
    }
    None
}
fn decorator_span(src: &str, decorator: &str) -> Option<(String, usize, usize)> {
    let m = mask(src);
    let tag = format!("@{decorator}");
    let at = m.find(&tag)?;
    let end = at + tag.len();
    let next = m[end..].trim_start();
    if !next.starts_with('(') { return None; }
    let paren = m[end..].find('(')? + end;
    let brace = m[paren..].find('{')? + paren;
    let close = matching(&m, brace, b'{', b'}')?;
    Some((m, brace, close))
}
// Find a top-level decorator property and return its value span.
fn property(masked: &str, open: usize, close: usize, name: &str) -> Option<(usize, usize)> {
    let b = masked.as_bytes();
    let mut start = open + 1;
    let (mut brace, mut square, mut round) = (0i32, 0i32, 0i32);
    for i in open + 1..=close {
        let c = if i == close { b',' } else { b[i] };
        if c == b',' && brace == 0 && square == 0 && round == 0 {
            let seg = &masked[start..i];
            let trim = seg.trim_start();
            let lead = seg.len() - trim.len();
            let k = start + lead;
            if keyword_at(masked, name, k) {
                let mut p = k + name.len();
                while p < i && b[p].is_ascii_whitespace() { p += 1; }
                if b.get(p) == Some(&b':') { return Some((p + 1, i)); }
            }
            start = i + 1;
        } else {
            match c {
                b'{' => brace += 1, b'}' => brace -= 1,
                b'[' => square += 1, b']' => square -= 1,
                b'(' => round += 1, b')' => round -= 1,
                _ => {}
            }
        }
    }
    None
}
fn unquote(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let delim = raw.chars().next()?;
    if delim != '\'' && delim != '"' { return None; }
    let tail = &raw[1..];
    let end = tail.find(delim)?;
    Some(tail[..end].to_owned())
}
fn class_name(src: &str, masked: &str, after_decorator: usize) -> Option<String> {
    // The decorated class appears AFTER the metadata. Avoid choosing another class
    // earlier in the file, which would generate a wrong custom pipe import.
    let rest = &masked[after_decorator..];
    let regular = rest.find("export class ").map(|at| (at, "export class ".len()));
    let default = rest.find("export default class ").map(|at| (at, "export default class ".len()));
    let (at, len) = match (regular, default) {
        (Some(a), Some(b)) => if a.0 < b.0 { a } else { b },
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => return None,
    };
    let candidate = &src[after_decorator + at + len..];
    let name: String = candidate.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$').collect();
    (!name.is_empty()).then_some(name)
}
pub fn component_meta(src: &str) -> Option<ComponentMeta> {
    let (masked, open, close) = decorator_span(src, "Component")?;
    let get_string = |name| property(&masked, open, close, name).and_then(|(a,b)| unquote(&src[a..b]));
    let standalone = property(&masked, open, close, "standalone")
        .and_then(|(a,b)| match masked[a..b].trim() { "true" => Some(true), "false" => Some(false), _ => None });
    Some(ComponentMeta {
        template_url: get_string("templateUrl"),
        standalone,
    })
}

/// Metadata of a project-local attribute directive. No selector is hard-coded.
pub fn directive_meta(src: &str) -> Option<DirectiveMeta> {
    let (masked, open, close) = decorator_span(src, "Directive")?;
    let (a, b) = property(&masked, open, close, "selector")?;
    let selector = unquote(&src[a..b])?;
    let standalone = property(&masked, open, close, "standalone")
        .and_then(|(a,b)| match masked[a..b].trim() { "true" => Some(true), "false" => Some(false), _ => None });
    Some(DirectiveMeta { selector, class_name: class_name(src, &masked, close), standalone })
}

/// Metadata for a project-local `@Pipe({ name: '...', standalone: true })`.
/// Angular 19+ defaults to standalone, previous versions require `standalone: true`.
pub fn pipe_meta(src: &str) -> Option<PipeMeta> {
    let (masked, open, close) = decorator_span(src, "Pipe")?;
    let (a, b) = property(&masked, open, close, "name")?;
    let name = unquote(&src[a..b])?;
    if name.is_empty() { return None; }
    let standalone = property(&masked, open, close, "standalone")
        .and_then(|(a,b)| match masked[a..b].trim() { "true" => Some(true), "false" => Some(false), _ => None });
    Some(PipeMeta { name, class_name: class_name(src, &masked, close), standalone })
}

#[derive(Clone, Debug)]
struct NamedImport {
    module: String,
    items_start: usize,
    items_end: usize,
    // A symbol may already be imported with `as Alias`.
    symbols: Vec<(String, String)>,
}
fn named_imports(src: &str) -> Vec<NamedImport> {
    let m = mask(src);
    let mut result = Vec::new();
    for (at, _) in m.match_indices("import") {
        if !keyword_at(&m, "import", at) { continue; }
        let tail = &m[at + 6..];
        let Some(brace_delta) = tail.find('{') else { continue };
        let open = at + 6 + brace_delta;
        // Avoid unrelated '{' from some later declaration.
        if m[at + 6..open].contains(';') || m[at + 6..open].contains("from")
            || m[at + 6..open].trim().starts_with("type") { continue; }
        let Some(close) = matching(&m, open, b'{', b'}') else { continue };
        let after = &m[close + 1..];
        let Some(from_delta) = after.find("from") else { continue };
        if from_delta > 16 { continue; }
        let from_at = close + 1 + from_delta;
        let Some(module) = unquote(&src[from_at + 4..]) else { continue };
        let symbols = src[open + 1..close].split(',').filter_map(|s| {
            let mut toks = s.split_whitespace();
            let imported = toks.next()?;
            let local = if toks.next() == Some("as") { toks.next().unwrap_or(imported) } else { imported };
            Some((imported.to_owned(), local.to_owned()))
        }).collect();
        result.push(NamedImport { module, items_start: open + 1, items_end: close, symbols });
    }
    result
}
/// A named import that is actually registered in the component's literal `imports` array.
/// `symbol` is the exported name, even when the import has an alias (`Foo as Bar`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveImport {
    pub module: String,
    pub symbol: String,
}

/// Only accept standalone component metadata; Angular < 19 needs explicit `standalone: true`.
/// This check keeps suggestions from appearing for NgModule-declared components, whose
/// dependencies cannot be added to `@Component.imports`.
pub fn supports_component_imports(src: &str, angular_major: Option<u32>) -> bool {
    let Some(meta) = component_meta(src) else { return false; };
    meta.standalone != Some(false)
        && (meta.standalone == Some(true) || !angular_major.is_some_and(|major| major < 19))
}

/// Import elements must be plain identifiers; we cannot safely inspect computed
/// expressions, spreads, nested arrays or constants. We deliberately do not guess.
fn literal_import_names(src: &str) -> BTreeSet<String> {
    let Some((masked, open, close)) = decorator_span(src, "Component") else { return BTreeSet::new(); };
    let Some((a,b)) = property(&masked, open, close, "imports") else { return BTreeSet::new(); };
    let Some(start) = masked[a..b].find('[').map(|v| a + v) else { return BTreeSet::new(); };
    let Some(end) = matching(&masked, start, b'[', b']') else { return BTreeSet::new(); };
    if end >= b || !masked[a..start].trim().is_empty() || !masked[end+1..b].trim().is_empty() {
        return BTreeSet::new();
    }
    let mut result = BTreeSet::new();
    for candidate in masked[start+1..end].split(',') {
        let name = candidate.trim();
        if !name.is_empty() && name.bytes().all(ident)
            && !name.as_bytes()[0].is_ascii_digit() {
            result.insert(name.to_owned());
        }
    }
    result
}

/// Only TypeScript symbols *and* their registration in the decorator count as
/// provided to the template. A top-level TS import alone does not count.
pub fn active_imports(src: &str) -> Vec<ActiveImport> {
    let names = literal_import_names(src);
    named_imports(src).into_iter().flat_map(|item| {
        item.symbols.into_iter().filter_map(|(symbol, local)| {
            names.contains(&local).then(|| ActiveImport { module: item.module.clone(), symbol })
        }).collect::<Vec<_>>()
    }).collect()
}

fn insert(edits: &mut BTreeMap<usize, String>, offset: usize, text: &str) {
    edits.entry(offset).or_default().push_str(text);
}
fn newline(src: &str) -> &'static str { if src.contains("\r\n") { "\r\n" } else { "\n" } }
fn line_indent(src: &str, at: usize) -> String {
    let start = src[..at].rfind('\n').map_or(0, |p| p + 1);
    src[start..at].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

/// Skip spaces and comments without skipping string literals: `mask` blanks
/// both comments *and* quoted module specifiers, which are distinct here.
fn skip_import_trivia(raw: &[u8], mut at: usize) -> usize {
    loop {
        while at < raw.len() && raw[at].is_ascii_whitespace() { at += 1; }
        if raw[at..].starts_with(b"//") {
            while at < raw.len() && raw[at] != b'\n' { at += 1; }
        } else if raw[at..].starts_with(b"/*") {
            at += 2;
            while at + 1 < raw.len() && !raw[at..].starts_with(b"*/") { at += 1; }
            at = (at + 2).min(raw.len());
        } else {
            return at;
        }
    }
}

/// Find the position immediately following the last static `import` statement,
/// preserving any header and existing import order. String literals and
/// comments have been masked, so an `import` in documentation never counts.
/// Dynamic `import(...)` calls and `import.meta` are intentionally ignored.
fn after_last_import(src: &str) -> Option<usize> {
    let masked = mask(src);
    let bytes = masked.as_bytes();
    let raw = src.as_bytes();
    let mut start = 0;
    let mut last = None;
    while start < src.len() {
        let line_end = src[start..].find('\n').map_or(src.len(), |n| start + n);
        let line = &masked[start..line_end];
        let leading = line.len() - line.trim_start().len();
        let at = start + leading;
        if keyword_at(&masked, "import", at) {
            let mut cursor = skip_import_trivia(raw, at + "import".len());
            // Only static import statements; skip `import('x')` and `import.meta`.
            if !matches!(bytes.get(cursor), Some(b'(' | b'.')) {
                let module_start = if matches!(raw.get(cursor), Some(b'\'' | b'"')) {
                    // `import 'side-effect';`
                    Some(cursor)
                } else {
                    // `import { Foo } from 'pkg'`, including multiline and `type` imports.
                    let limit = src.len().min(at.saturating_add(8192));
                    let mut from = None;
                    while cursor + 4 <= limit {
                        if keyword_at(&masked, "from", cursor) {
                            from = Some(cursor + 4);
                            break;
                        }
                        // A terminated statement without `from` is not an ES import.
                        if bytes[cursor] == b';' { break; }
                        cursor += 1;
                    }
                    from.and_then(|pos| {
                        let pos = skip_import_trivia(raw, pos);
                        matches!(raw.get(pos), Some(b'\'' | b'"')).then_some(pos)
                    })
                };
                if let Some(open_quote) = module_start {
                    let quote = raw[open_quote];
                    let mut end = open_quote + 1;
                    while end < raw.len() {
                        if raw[end] == b'\\' { end = (end + 2).min(raw.len()); continue; }
                        if raw[end] == quote { end += 1; break; }
                        end += 1;
                    }
                    // Place new declarations at the beginning of the next line;
                    // keep any trailing comment attached to the old import.
                    let insertion = src[end..].find('\n')
                        .map_or(src.len(), |n| end + n + 1);
                    last = Some(last.map_or(insertion, |prev: usize| prev.max(insertion)));
                }
            }
        }
        start = if line_end < src.len() { line_end + 1 } else { src.len() };
    }
    last
}

/// UTF-8 BOM and shebang must remain at the start even without existing imports.
fn import_fallback(src: &str) -> usize {
    let mut at = if src.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    if src[at..].starts_with("#!") {
        at = src[at..].find('\n').map(|n| at + n + 1).unwrap_or(src.len());
    }
    at
}

fn already_in_array(imports: &str, local: &str) -> bool {
    let m = mask(imports);
    word_in(&m, local)
}

/// Add dependencies to `@Component({ imports: [...] })`, or add that property.
/// `angular_major` disallows implicit-standalone components before Angular 19.
/// Returns None if every required dependency is already registered.
pub fn add_imports(src: &str, dependencies: &[Dependency], angular_major: Option<u32>) -> Result<Option<String>, String> {
    let (m, open, close) = decorator_span(src, "Component").ok_or("@Component nao encontrado")?;
    let standalone = property(&m, open, close, "standalone").and_then(|(a,b)| {
        match m[a..b].trim() { "true" => Some(true), "false" => Some(false), _ => None }
    });
    if standalone == Some(false) || (standalone.is_none() && angular_major.is_some_and(|major| major < 19)) {
        return Err("O componente pertence a um NgModule (nao e standalone); alteracoes automaticas foram bloqueadas".into());
    }
    let original_field = property(&m, open, close, "imports");
    let array = original_field.and_then(|(a, b)| {
        let start = m[a..b].find('[')? + a;
        let end = matching(&m, start, b'[', b']')?;
        // Only edit a literal imports array, not computed expressions.
        (end < b && m[a..start].trim().is_empty() && m[end + 1..b].trim().is_empty()).then_some((start, end))
    });
    if original_field.is_some() && array.is_none() {
        return Err("imports do @Component nao e um array literal; correcao automatica cancelada".into());
    }
    let imports = named_imports(src);
    let old_array = array.map(|(a,b)| &src[a + 1..b]).unwrap_or("");
    if old_array.contains("/*") || old_array.contains("//") {
        return Err("O array imports contem comentarios; edicao automatica desabilitada para preservar comentarios".into());
    }
    let mut new_members = Vec::<String>::new();
    let mut new_modules = BTreeMap::<String, Vec<String>>::new();
    let mut existing_import_additions = BTreeMap::<usize, Vec<String>>::new();
    for dep in dependencies {
        // A CommonModule or RouterModule already listed satisfies many directives.
        if dep.covered_by.as_deref().is_some_and(|group| already_in_array(old_array, group)) { continue; }
        let imported = imports.iter()
            .filter(|im| im.module == dep.module)
            .flat_map(|im| &im.symbols)
            .find(|(name, _)| name == &dep.symbol);
        let local = imported.map(|(_, alias)| alias.clone()).unwrap_or_else(|| dep.symbol.clone());
        if already_in_array(old_array, &local) || new_members.contains(&local) { continue; }
        new_members.push(local);
        if imported.is_none() {
            if let Some(existing) = imports.iter().find(|im| im.module == dep.module) {
                let inside = &src[existing.items_start..existing.items_end];
                let pos = existing.items_start + inside.trim_end().len();
                existing_import_additions.entry(pos).or_default().push(dep.symbol.clone());
            } else {
                new_modules.entry(dep.module.clone()).or_default().push(dep.symbol.clone());
            }
        }
    }
    if new_members.is_empty() { return Ok(None); }
    let mut edits = BTreeMap::<usize, String>::new();
    for (at, symbols) in existing_import_additions {
        let inside = imports.iter().find(|im| im.items_start <= at && at <= im.items_end).expect("matching import");
        let current = src[inside.items_start..at].trim();
        let prefix = if current.is_empty() { "" } else if current.ends_with(',') { " " } else { ", " };
        insert(&mut edits, at, &format!("{}{}", prefix, symbols.join(", ")));
    }
    if !new_modules.is_empty() {
        let mut text = String::new();
        for (module, symbols) in new_modules {
            text.push_str(&format!("import {{ {} }} from '{}';{}", symbols.join(", "), module, newline(src)));
        }
        // Append after the final existing static import, not before the first.
        // When that import has no trailing newline, separate the statements.
        let at = after_last_import(src).unwrap_or_else(|| import_fallback(src));
        if at > 0 && !src[..at].ends_with('\n') {
            text.insert_str(0, newline(src));
        }
        insert(&mut edits, at, &text);
    }
    if let Some((arr_open, arr_close)) = array {
        let inner = &src[arr_open + 1..arr_close];
        if inner.trim().is_empty() {
            insert(&mut edits, arr_close, &new_members.join(", "));
        } else if !inner.contains('\n') {
            let sep = if inner.trim_end().ends_with(',') { " " } else { ", " };
            insert(&mut edits, arr_close, &format!("{sep}{}", new_members.join(", ")));
        } else {
            // Respect multiline indentation, only append members.
            let close_line_start = src[..arr_close].rfind('\n').map_or(arr_open + 1, |n| n + 1);
            let before_close = &src[close_line_start..arr_close];
            if !before_close.trim().is_empty() {
                // A multiline array may close on the same line as its last item:
                // imports: [\n  RouterOutlet] (valid TypeScript).
                // Append beside that item rather than rejecting the Quick Fix.
                // Keeping the insertion immediately after the last non-space
                // byte also preserves any whitespace before the closing bracket.
                let last_end = arr_open + 1 + inner.trim_end().len();
                let last = inner.trim_end();
                let sep = if last.ends_with(',') { " " } else { ", " };
                insert(&mut edits, last_end, &format!("{sep}{}", new_members.join(", ")));
            } else {
                let last = src[arr_open + 1..close_line_start].trim_end();
                if !last.ends_with(',') {
                    let last_pos = arr_open + 1 + src[arr_open + 1..close_line_start].trim_end().len();
                    insert(&mut edits, last_pos, ",");
                }
                let close_indent = line_indent(src, arr_close);
                let item_indent = format!("{close_indent}  ");
                let addition = new_members.iter()
                    .map(|v| format!("{item_indent}{v},{}", newline(src)))
                    .collect::<String>();
                insert(&mut edits, close_line_start, &addition);
            }
        }
    } else {
        let member_line = new_members.join(", ");
        let multiline = src[open..close].contains('\n');
        let field_indent = if multiline {
            let next = src[open + 1..close].lines().find(|s| !s.trim().is_empty()).unwrap_or("  ");
            next.chars().take_while(|c| *c == ' ' || *c == '\t').collect::<String>()
        } else { String::new() };
        let intro = if multiline {
            format!("{}{field_indent}imports: [{member_line}],", newline(src))
        } else { format!("imports: [{member_line}], ") };
        insert(&mut edits, open + 1, &intro);
    }
    let mut result = src.to_owned();
    for (at, text) in edits.into_iter().rev() {
        result.insert_str(at, &text);
    }
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn router() -> Dependency { Dependency::new("RouterLink", "@angular/router", Some("RouterModule")) }
    #[test]
    fn add_standalone_imports_and_idempotency() {
        let src = "import { Component } from '@angular/core';\n@Component({\n  selector: 'app-test',\n  templateUrl: './test.html'\n})\nexport class TestComponent {}\n";
        let updated = add_imports(src, &[router()], Some(19)).unwrap().unwrap();
        assert!(updated.contains("import { RouterLink } from '@angular/router';"));
        assert!(updated.contains("imports: [RouterLink]"));
        assert!(add_imports(&updated, &[router()], Some(19)).unwrap().is_none());
    }
    #[test]
    fn adds_to_existing_router_import() {
        let src = "import { RouterOutlet } from '@angular/router';\n@Component({ imports: [RouterOutlet] })\nexport class A {}";
        let out = add_imports(src, &[router()], Some(20)).unwrap().unwrap();
        assert!(out.contains("RouterOutlet, RouterLink"));
        assert_eq!(out.matches("from '@angular/router'").count(), 1);
    }
    #[test]
    fn module_import_satisfies_directive() {
        let src = "@Component({standalone: true, imports: [RouterModule]}) export class A {}";
        assert!(add_imports(src, &[router()], None).unwrap().is_none());
    }
    #[test]
    fn blocks_ngmodule_component() {
        let src = "@Component({standalone: false, templateUrl: './x.html'}) export class A {}";
        assert!(add_imports(src, &[router()], Some(19)).is_err());
    }
    #[test]
    fn adds_ngx_mask_pipe_without_duplicate_imports() {
        let src = "import { Component } from '@angular/core';\n@Component({ standalone: true, imports: [RouterLink] })\nexport class C {}";
        let dep = Dependency::new("NgxMaskPipe", "ngx-mask", None);
        let out = add_imports(src, &[dep.clone()], Some(19)).unwrap().unwrap();
        assert!(out.contains("import { NgxMaskPipe } from 'ngx-mask';"));
        assert!(out.contains("RouterLink, NgxMaskPipe"));
        assert!(add_imports(&out, &[dep], Some(19)).unwrap().is_none());
    }
    #[test]
    fn finds_pipe_class_after_its_decorator() {
        let src = "export class Earlier {}\n@Pipe({ name: 'zip' })\nexport class ZipPipe {}";
        assert_eq!(pipe_meta(src).unwrap().class_name.as_deref(), Some("ZipPipe"));
    }
    #[test]
    fn merges_ngx_mask_import_with_existing_module_import() {
        let src = "import { NgxMaskDirective } from 'ngx-mask';\n@Component({ imports: [NgxMaskDirective] }) export class C {}";
        let dep = Dependency::new("NgxMaskPipe", "ngx-mask", None);
        let out = add_imports(src, &[dep], Some(20)).unwrap().unwrap();
        assert_eq!(out.matches("from 'ngx-mask'").count(), 1);
        assert!(out.contains("NgxMaskDirective, NgxMaskPipe"));
    }
    #[test]
    fn detects_custom_pipe_metadata() {
        let src = "@Pipe({ name: 'phoneFormat', standalone: true })\nexport class PhoneFormatPipe {}";
        let meta = pipe_meta(src).unwrap();
        assert_eq!(meta.name, "phoneFormat");
        assert_eq!(meta.class_name.as_deref(), Some("PhoneFormatPipe"));
        assert_eq!(meta.standalone, Some(true));
    }
    #[test]
    fn gets_component_metadata() {
        let src = "@Component({ selector: 'app-card', templateUrl: '../views/card.html', standalone: true }) export class CardComponent {}";
        let m = component_meta(src).unwrap();
        assert_eq!(m.template_url.as_deref(), Some("../views/card.html"));
        assert_eq!(m.standalone, Some(true));
    }
    #[test]
    fn adds_after_last_item_when_multiline_array_closes_on_the_same_line() {
        let src = "import { RouterOutlet } from '@angular/router';\n@Component({\n  imports: [\n    RouterOutlet]\n})\nexport class Demo {}";
        let out = add_imports(src, &[router()], Some(20)).unwrap().unwrap();
        assert!(out.contains("RouterOutlet, RouterLink]"));
        assert_eq!(out.matches("from '@angular/router'").count(), 1);
        assert!(add_imports(&out, &[router()], Some(20)).unwrap().is_none());
    }
    #[test]
    fn adds_after_trailing_comma_when_multiline_array_closes_on_the_same_line() {
        let src = "import { RouterOutlet } from '@angular/router';\n@Component({\n  imports: [\n    RouterOutlet, ]\n})\nexport class Demo {}";
        let out = add_imports(src, &[router()], Some(20)).unwrap().unwrap();
        assert!(out.contains("RouterOutlet, RouterLink ]"));
    }
    #[test]
    fn keeps_multiline_array_with_closing_bracket_on_its_own_line() {
        let src = "@Component({\n  imports: [\n    Foo\n  ]\n}) export class Demo {}";
        let out = add_imports(src, &[router()], Some(20)).unwrap().unwrap();
        assert!(out.contains("    Foo,\n    RouterLink,\n  ]"));
    }
    #[test]
    fn appends_new_import_after_last_existing_import() {
        let src = "import { Component } from '@angular/core';\nimport { BehaviorSubject } from 'rxjs';\n\n@Component({ imports: [] })\nexport class C {}";
        let out = add_imports(src, &[router()], Some(20)).unwrap().unwrap();
        let subject = out.find("from 'rxjs'").unwrap();
        let link = out.find("import { RouterLink } from '@angular/router';").unwrap();
        let component = out.find("@Component").unwrap();
        assert!(subject < link && link < component);
        assert!(out.contains("import { BehaviorSubject } from 'rxjs';\nimport { RouterLink } from '@angular/router';\n\n@Component"));
        assert!(add_imports(&out, &[router()], Some(20)).unwrap().is_none());
    }
    #[test]
    fn appends_after_multiline_and_type_imports() {
        let src = "import type {\n  SomeType\n} from './types'\nimport { Component } from '@angular/core'; // comment\n\n@Component({ imports: [] }) export class C {}";
        let out = add_imports(src, &[router()], Some(20)).unwrap().unwrap();
        assert!(out.contains("from '@angular/core'; // comment\nimport { RouterLink } from '@angular/router';\n"));
    }
    #[test]
    fn appends_after_side_effect_import() {
        let src = "import { Component } from '@angular/core';\nimport 'zone.js';\n@Component({ imports: [] }) export class C {}";
        let out = add_imports(src, &[router()], Some(20)).unwrap().unwrap();
        assert!(out.contains("import 'zone.js';\nimport { RouterLink } from '@angular/router';\n@Component"));
    }
    #[test]
    fn adds_import_after_last_line_with_no_final_newline() {
        let src = "import { Component } from '@angular/core'";
        let at = after_last_import(src).unwrap();
        assert_eq!(at, src.len());
        // This helper's position needs a separating newline before inserting.
        assert!(!src[..at].ends_with('\n'));
    }
    #[test]
    fn no_existing_imports_still_keeps_bom_and_shebang() {
        let src = "\u{feff}#!/usr/bin/env node\nconsole.log('test');\n";
        assert_eq!(after_last_import(src), None);
        assert_eq!(import_fallback(src), "\u{feff}#!/usr/bin/env node\n".len());
    }
    #[test]
    fn ignores_dynamic_imports_and_comments() {
        let src = "// import { Fake } from 'fake';\nconst value = import('external');\nimport.meta;\nimport { Real } from 'real';\n";
        assert_eq!(after_last_import(src), Some(src.len()));
        let src = "// import { Fake } from 'fake';\nconst value = import('external');\n";
        assert_eq!(after_last_import(src), None);
    }
    #[test]
    fn preserves_crlf_import_blocks() {
        let src = "import { Component } from '@angular/core';\r\n\r\n@Component({ imports: [] }) export class C {}";
        let out = add_imports(src, &[router()], Some(20)).unwrap().unwrap();
        assert!(out.contains("from '@angular/core';\r\nimport { RouterLink } from '@angular/router';\r\n\r\n@Component"));
    }
    #[test]
    fn only_registered_named_imports_are_active() {
        let src = r#"import { RouterLink as RL, RouterModule } from '@angular/router';
import { NgxMaskPipe, NgxMaskDirective as MaskDir } from 'ngx-mask';
@Component({ standalone: true, imports: [RL, NgxMaskPipe] })
export class Demo {}"#;
        let active = active_imports(src);
        assert_eq!(active.len(), 2);
        assert!(active.contains(&ActiveImport { module: "@angular/router".into(), symbol: "RouterLink".into() }));
        assert!(active.contains(&ActiveImport { module: "ngx-mask".into(), symbol: "NgxMaskPipe".into() }));
        assert!(!active.iter().any(|v| v.symbol == "RouterModule" || v.symbol == "NgxMaskDirective"));
    }
    #[test]
    fn ignores_imports_in_comments_and_computed_arrays() {
        let src = r#"import { RouterLink } from '@angular/router';
@Component({imports: [/* RouterLink */ other]}) export class A {}"#;
        assert!(active_imports(src).is_empty());
        let src = r#"import { RouterLink } from '@angular/router';
@Component({imports: SOME_IMPORTS}) export class A {}"#;
        assert!(active_imports(src).is_empty());
    }
    #[test]
    fn ngmodule_components_do_not_accept_direct_imports() {
        assert!(!supports_component_imports("@Component({standalone: false}) class C {}", Some(20)));
        assert!(!supports_component_imports("@Component({}) class C {}", Some(18)));
        assert!(supports_component_imports("@Component({standalone: true}) class C {}", Some(18)));
        assert!(supports_component_imports("@Component({}) class C {}", Some(20)));
    }

}
