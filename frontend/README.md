# Operator UI

Repository-local React operator surface for the standalone browser-profile control plane.

- Runtime: Node.js 24.19.0 LTS, npm 11.17.0.
- API: same-origin `/api/v1/*` only.
- Remote state: TanStack Query.
- Routing: TanStack Router.
- Authorization and lifecycle decisions remain in the Rust Worker/domain core.
- No credential material is persisted in Web Storage.
- `.github/workflows/frontend-gate.yml` is the read-only acceptance lane for clean install, strict TypeScript, unit tests, production build, source credential-persistence scanning, and Static Assets output.

Build commands:

```bash
npm ci
npm run typecheck
npm test
npm run build
```

The Worker Static Assets binding serves `frontend/dist`. Unknown `/api/*`, `/auth/*`, and `/bridge/*` paths remain fail-closed in the Worker route classifier and must never fall through to SPA assets.
