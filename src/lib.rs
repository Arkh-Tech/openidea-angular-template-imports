//! Angular Template Imports for OpenIDEA.
//! HTML quick fixes are lazy: project scans and TS modifications happen on selection.
mod angular;
mod metadata;
mod packages;
mod resolver;
mod template;
mod typescript;

use openidea_plugin::{host, register_plugin, text, Error, Invocation, Plugin, Range, TextEdit};
use openidea_plugin::providers::{CodeAction, Document, Providers};

#[derive(Default)]
pub struct AngularTemplateImports;

impl Plugin for AngularTemplateImports {
    fn run_command(&mut self, id: &str, _ctx: &Invocation, _arg: Option<&str>) -> Result<Option<String>, Error> {
        Err(Error::NotFound(format!("comando desconhecido: {id}")))
    }
}

fn action(title: String, data: String) -> CodeAction {
    CodeAction { title, kind: Some("quickfix".into()), preferred: false, edits: Vec::new(), data: Some(data) }
}

impl Providers for AngularTemplateImports {
    fn code_actions(&mut self, doc: &Document, _selection: Range) -> Vec<CodeAction> {
        if !doc.path.ends_with(".html") { return Vec::new(); }
        let needs = template::detect(&doc.text);
        if needs.is_empty() { return Vec::new(); }

        // Read the actual owning component *before* proposing a quick fix.
        // Only symbols registered in @Component.imports can satisfy the template.
        // The metadata lookup visits only packages already registered there; the
        // expensive scan for new imports is still deferred to resolve_code_action.
        let Some(project) = host::project_of(&doc.path) else { return Vec::new(); };
        let Ok((ts_path, component)) = resolver::owner(&project, &doc.path) else { return Vec::new(); };
        if !typescript::supports_component_imports(&component, resolver::angular_major(&project)) {
            return Vec::new();
        }
        let satisfied = resolver::satisfied_needs(&project, &ts_path, &component, &needs);
        let missing: Vec<_> = needs.into_iter()
            .filter(|need| !satisfied.contains(&need.id())).collect();
        if missing.is_empty() { return Vec::new(); }

        let mut result = Vec::new();
        for need in &missing {
            result.push(action(format!("Angular: localizar importação de {}", need.label()), need.id()));
        }
        if missing.len() > 1 {
            let mut all = action("Angular: importar diretivas e pipes encontrados".into(), "all".into());
            all.preferred = true;
            result.insert(0, all);
        }
        result
    }

    fn resolve_code_action(&mut self, doc: &Document, mut action: CodeAction) -> Result<CodeAction, Error> {
        let id = action.data.as_deref().ok_or_else(|| Error::Invalid("Acao sem identificador".into()))?;
        let needs = template::detect(&doc.text);
        let selected: Vec<_> = if id == "all" {
            needs
        } else {
            needs.into_iter().filter(|need| need.id() == id).collect()
        };
        if selected.is_empty() { return Err(Error::Stale); }
        let project = host::project_of(&doc.path).ok_or_else(|| Error::NotFound("Projeto Angular".into()))?;
        let (ts_path, source) = resolver::owner(&project, &doc.path)?;
        let major = resolver::angular_major(&project);
        if !typescript::supports_component_imports(&source, major) { return Err(Error::Stale); }
        // Recheck at the time of clicking, too: an action can become stale if
        // another fix or a manual edit already registered this dependency.
        let satisfied = resolver::satisfied_needs(&project, &ts_path, &source, &selected);
        let missing: Vec<_> = selected.into_iter()
            .filter(|need| !satisfied.contains(&need.id())).collect();
        if missing.is_empty() { return Err(Error::Stale); }
        let deps = resolver::dependencies(&project, &ts_path, &missing, id != "all")?;
        if deps.is_empty() { action.data = None; return Ok(action); }
        let changed = typescript::add_imports(&source, &deps, major).map_err(Error::Invalid)?;
        if let Some(changed) = changed {
            // One full-file replacement: the host handles this as one undoable workspace edit.
            let range = Range::new(text::position(&source, 0), text::position(&source, source.len()));
            action.edits = vec![(ts_path, vec![TextEdit { range, text: changed }])];
        }
        action.data = None;
        Ok(action)
    }
}

register_plugin!(AngularTemplateImports, providers);
