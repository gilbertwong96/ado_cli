# github-page/

Static landing page for [gilbertwong96.github.io/ado_cli](https://gilbertwong96.github.io/ado_cli/).

## Structure

- `index.html` — single-page site (no build step)
- `style.css` — hand-written CSS, dark theme, mobile-responsive
- `.nojekyll` — tells GitHub Pages to serve files as-is (no Jekyll)

## How it deploys

`.github/workflows/pages.yml` runs on every push to `main` that touches
`github-page/**`. It uses the official `actions/deploy-pages@v4` action
to upload `github-page/` as a Pages artifact and deploy it to the
`github-pages` environment.

No build step. Edit HTML/CSS in place, commit, push → live in ~30s.

## Local preview

```bash
cd github-page
python3 -m http.server 8000
# Open http://localhost:8000
```

## When updating

Keep the page in sync with the README's:

- "Why AI-native?" section
- Install instructions
- Skill count and target list
- Auth methods

If you add a new auth method or LLM agent target, update both
`README.md` and `github-page/index.html`.
