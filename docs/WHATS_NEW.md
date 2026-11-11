# What's New — Angular Template Imports v0.3.5

**Release focus:** More control over Angular imports, clearer alternatives, and better handling of public aliases.

## New features

### Multiple import choices

When a directive or pipe has multiple valid import origins, the plugin can now offer each public symbol and package entry point as a separate choice. You no longer have to rely on a single automatically selected name.

### Interactive import picker

Use **Angular: escolher importação para ...** to choose an origin in a picker. If only one valid origin exists, the plugin can apply it directly without opening the picker.

### Previewable Quick Fixes

Use **Angular: localizar opções para visualizar prévias** to discover candidates on demand. Reopen the Intentions menu to see concrete actions with native previews of the proposed TypeScript edits, where supported.

### Explicit test/testing alternatives

Exports from `test` or `testing` entry points can appear as optional choices labeled **(teste)**. Production imports remain preferred, and testing-only imports are not automatically selected.

## Improvements and fixes

- **Preserved public aliases:** declarations can expose multiple public names, such as `RouterLink` and `RouterLinkWithHref`, without one overwriting the other.
- **Improved re-export resolution:** public names are followed through supported package export chains instead of being collapsed to a single alias.
- **Cache refresh:** **Angular: atualizar opções de importação** refreshes the candidate list after metadata or dependency changes.
- **Import ordering:** new TypeScript import statements continue to be appended after existing imports.
- **Duplicate protection:** avoid re-adding symbols already imported or registered in `@Component.imports`.
- **Bulk actions:** preserve the ability to address multiple missing template dependencies while selecting the appropriate origin for ambiguous cases.

## Compatibility

- Plugin ID: `arkhtech.angular-template-imports`
- OpenIDEA plugin API requirement: `^1.3`
- SDK baseline: OpenIDEA `v0.1.39`
- The changes are based on the stable v0.3.4 code rather than the experimental semantic-highlighting branch.

## Known limitations

- Component-tag auto-import and Angular-specific `signal`/component semantic coloring are **not** included.
- The picker is not the native nested TypeScript import submenu.
- A first discovery step is needed before showing previewable per-origin options.
- Available imports depend on the Angular libraries and metadata installed in the project.

---

**Maintained by [Arkh-Tech](https://github.com/Arkh-Tech).**
[Source code](https://github.com/Arkh-Tech/openidea-angular-template-imports) · [Releases](https://github.com/Arkh-Tech/openidea-angular-template-imports/releases) · [Report an issue](https://github.com/Arkh-Tech/openidea-angular-template-imports/issues)
