# Angular Template Imports — Overview

**Angular Template Imports** adds helpful Angular Quick Fixes to OpenIDEA, so you can resolve missing directives and pipes from HTML templates without manually editing every standalone component import.

The plugin analyzes the template and checks Angular declarations from the current project and installed dependencies. When it finds a compatible public declaration, it can add the required TypeScript import and update the owning component's `imports` array.

## Highlights

- Quick Fixes for Angular attribute directives, structural directives and pipes.
- Dependency resolution based on project source and installed Angular library metadata, rather than hardcoded mappings.
- Supports common Angular Router, Forms and compatible third-party declarations when the required metadata is available.
- Keeps TypeScript imports organized by appending new statements after the current import block.
- Avoids duplicate component imports and filters testing-related packages when suggesting new dependencies.
- Supports an import-all action for multiple compatible missing dependencies.

## Typical workflow

Open an Angular component template, use a directive such as `routerLink`, and select the Angular Quick Fix when the dependency is missing. The plugin finds the matching installed declaration, then prepares the component import update for you.

## Compatibility and limitations

Requires an OpenIDEA build that supports Plugin API `^1.3`. Angular dependencies must be installed in the project. This release focuses on **directives and pipes** and **standalone component imports**; it does not auto-import component tags and does not include stable Angular-specific syntax coloring for components or signals.

**Publisher:** Arkh-Tech
**Plugin ID:** `arkhtech.angular-template-imports`
**Recommended release:** `0.3.3`
**Source repository (planned):** https://github.com/Arkh-Tech/openidea-angular-template-imports
