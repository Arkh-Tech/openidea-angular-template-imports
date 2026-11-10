# What's New — v0.3.3

This release improves Angular Quick Fix accuracy and keeps generated TypeScript imports in the expected location.

## Improved dependency resolution

- Excludes testing-related entry points such as `@angular/router/testing`, `test`, `tests`, `spec`, `fixtures`, `__tests__`, and `e2e` from **new** dependency suggestions.
- Excludes test directories and `*.spec.ts` / `*.test.ts` files when scanning local Angular directive and pipe declarations.
- Reduces false suggestions from test-only package exports, including `routerLink` lookalikes.

## Better TypeScript imports

- Places new `import` statements **after the existing import block**, not at the top of the file.
- Handles common multiline imports, `import type`, side-effect imports, CRLF line endings and trailing comments.
- Preserves behavior that adds symbols to an existing import from the same package when possible.
- Prevents duplicate dependency registration in standalone component imports.

## Reliability

- Adds regression tests for test-package filtering and import ordering.
- Keeps the conservative behavior for ambiguous Angular declarations.

## Current scope

This version provides template Quick Fixes for **Angular directives and pipes**. Automatic imports of custom component tags and green semantic highlighting of Angular components/signals are **not yet supported** as stable features.
